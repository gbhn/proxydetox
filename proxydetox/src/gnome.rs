use detox_net::PathOrUri;
use gio::prelude::*;
use std::str::FromStr;
use tokio::sync::mpsc;

const GSETTINGS_PROXY_SCHEMA: &str = "org.gnome.system.proxy";
const GSETTINGS_KEY_MODE: &str = "mode";
const GSETTINGS_KEY_AUTOCONFIG_URL: &str = "autoconfig-url";

/// Запускает мониторинг настроек прокси в GSettings и отправляет URL для PAC-файла через канал.
///
/// Эта функция запускает отдельный поток для работы с циклом событий GLib,
/// так как он блокирующий и несовместим с асинхронным рантаймом Tokio напрямую.
pub fn monitor_gsettings_proxy() -> Result<mpsc::Receiver<Option<PathOrUri>>, glib::Error> {
    let (tx, rx) = mpsc::channel(8);

    tokio::task::spawn_blocking(move || {
        let main_context = glib::MainContext::default();
        let _guard = main_context.acquire().expect("Couldn't acquire GLib main context.");

        let settings = gio::Settings::new(GSETTINGS_PROXY_SCHEMA);

        let tx_clone = tx.clone();
        let settings_clone = settings.clone();

        // Функция для чтения текущих настроек и отправки их в канал
        let check_and_send = move || {
            let mode = settings_clone.string(GSETTINGS_KEY_MODE);
            if mode.as_str() == "auto" {
                let url_str = settings_clone.string(GSETTINGS_KEY_AUTOCONFIG_URL);
                if !url_str.is_empty() {
                    tracing::info!(wpad_url = %url_str, "GNOME WPAD setting found");
                    if let Ok(path_or_uri) = PathOrUri::from_str(&url_str) {
                        let _ = tx_clone.blocking_send(Some(path_or_uri));
                        return;
                    }
                }
            }
            tracing::info!("GNOME WPAD setting is not 'auto' or URL is empty");
            let _ = tx_clone.blocking_send(None);
        };

        // Проверяем и отправляем начальные значения
        check_and_send();

        // Подписываемся на изменения ключей 'mode' и 'autoconfig-url'
        settings.connect_changed(Some(GSETTINGS_KEY_MODE), move |_, _| {
            check_and_send();
        });
        settings.connect_changed(Some(GSETTINGS_KEY_AUTOCONFIG_URL), move |_, _| {
            check_and_send();
        });

        // Запускаем цикл GLib для обработки сигналов
        let main_loop = glib::MainLoop::new(Some(&main_context), false);
        main_loop.run();
    });

    Ok(rx)
}