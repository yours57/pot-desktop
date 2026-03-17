use futures_util::StreamExt;
use tauri::Manager;
use zbus::{Connection, MatchRule, MessageStream, MessageType};

// ---------------------------------------------------------------------------
// One-shot query: read current portal color-scheme value
// ---------------------------------------------------------------------------

fn parse_color_scheme(value: &zbus::zvariant::Value<'_>) -> Option<u32> {
    match value {
        zbus::zvariant::Value::U32(n) => Some(*n),
        // Handle implementations that wrap the value in an extra variant layer
        zbus::zvariant::Value::Value(inner) => parse_color_scheme(&**inner),
        _ => None,
    }
}

async fn do_query() -> zbus::Result<String> {
    let conn = Connection::session().await?;
    let proxy: zbus::Proxy<'_> = zbus::ProxyBuilder::new_bare(&conn)
        .destination("org.freedesktop.portal.Desktop")?
        .path("/org/freedesktop/portal/desktop")?
        .interface("org.freedesktop.portal.Settings")?
        .build()
        .await?;
    let result: zbus::zvariant::OwnedValue = proxy
        .call("Read", &("org.freedesktop.appearance", "color-scheme"))
        .await?;
    Ok(match parse_color_scheme(&*result) {
        Some(1) => "dark",
        Some(2) => "light",
        _ => "no-preference",
    }
    .to_string())
}

pub async fn query_color_scheme() -> String {
    match do_query().await {
        Ok(v) => v,
        Err(e) => {
            log::warn!("linux_theme: failed to query portal color-scheme: {}", e);
            "no-preference".to_string()
        }
    }
}

async fn inner(app: tauri::AppHandle) -> zbus::Result<()> {
    let conn = Connection::session().await?;

    let rule = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .interface("org.freedesktop.portal.Settings")?
        .member("SettingChanged")?
        .build();

    let mut stream = MessageStream::for_match_rule(rule, &conn, None).await?;

    while let Some(msg) = stream.next().await {
        let msg = msg?;
        let (namespace, key, value): (String, String, zbus::zvariant::OwnedValue) =
            msg.body()?;

        if namespace == "org.freedesktop.appearance" && key == "color-scheme" {
            if let zbus::zvariant::Value::U32(scheme) = &*value {
                // 1 = dark, 2 = light, 0 = no preference
                let theme = match scheme {
                    1 => "dark",
                    2 => "light",
                    _ => continue,
                };
                let _ = app.emit_all("system-theme-changed", theme);
                log::info!("linux_theme: system color-scheme changed to {}", theme);
            }
        }
    }

    Ok(())
}

pub async fn watch_system_theme(app: tauri::AppHandle) {
    if let Err(e) = inner(app).await {
        log::warn!("linux_theme: D-Bus theme watcher stopped: {}", e);
    }
}
