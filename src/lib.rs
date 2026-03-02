use aviutl2::{config::translate as tr, tracing};
use discord_rich_presence::DiscordIpc;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Config {
    enabled: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self { enabled: true }
    }
}

static CONFIG_PATH: std::sync::LazyLock<std::path::PathBuf> = std::sync::LazyLock::new(|| {
    process_path::get_executable_path()
        .unwrap()
        .with_file_name("discord_activity.aux2.json")
});

#[aviutl2::plugin(GenericPlugin)]
struct DiscordActivityAux2 {
    is_connected: bool,
    client: discord_rich_presence::DiscordIpcClient,
    started_at: time::OffsetDateTime,
    config: Config,
}

impl aviutl2::generic::GenericPlugin for DiscordActivityAux2 {
    fn new(_info: aviutl2::AviUtl2Info) -> aviutl2::AnyResult<Self> {
        aviutl2::tracing_subscriber::fmt()
            .with_max_level(if cfg!(debug_assertions) {
                aviutl2::tracing::Level::DEBUG
            } else {
                aviutl2::tracing::Level::INFO
            })
            .event_format(aviutl2::logger::AviUtl2Formatter)
            .with_writer(aviutl2::logger::AviUtl2LogWriter)
            .init();

        let config = match std::fs::read_to_string(&*CONFIG_PATH) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(config) => {
                    tracing::info!("Loaded config: {config:?}");
                    config
                }
                Err(e) => {
                    tracing::error!("Failed to parse config file: {e}");
                    Config::default()
                }
            },
            Err(e) => {
                tracing::error!("Failed to read config file: {e}");
                Config::default()
            }
        };
        Ok(Self {
            is_connected: false,
            client: discord_rich_presence::DiscordIpcClient::new("1478025726056857640"),
            started_at: time::OffsetDateTime::now_utc(),
            config,
        })
    }

    fn plugin_info(&self) -> aviutl2::generic::GenericPluginTable {
        aviutl2::generic::GenericPluginTable {
            name: "discord_activity.aux2".to_string(),
            information: format!(
                "Discord Rich Presence for AviUtl2 / v{} / https://github.com/sevenc-nanashi/discord_activity.aux2",
                env!("CARGO_PKG_VERSION")
            ),
        }
    }

    fn on_project_load(&mut self, _project: &mut aviutl2::generic::ProjectFile) {
        tracing::info!("Project loaded, updating Discord activity");
        self.started_at = time::OffsetDateTime::now_utc();
        self.update_activity();
    }

    fn register(&mut self, registry: &mut aviutl2::generic::HostAppHandle) {
        registry.register_menus::<Self>();
    }
}

impl DiscordActivityAux2 {
    fn update_activity(&mut self) {
        if self.config.enabled {
            if let Err(e) = self.set_activity() {
                tracing::error!("Failed to update Discord activity: {e}");
            }
        } else if let Err(e) = self.clear_activity() {
            tracing::error!("Failed to clear Discord activity: {e}");
        }
    }
    fn ensure_connected(&mut self) -> aviutl2::AnyResult<()> {
        if self.is_connected {
            tracing::info!("Pinging Discord IPC");
            match self.client.send(serde_json::json!({}), u8::MAX) {
                Ok(_) => {
                    tracing::info!("Discord IPC connection is healthy");
                }
                Err(
                    discord_rich_presence::error::Error::NotConnected
                    | discord_rich_presence::error::Error::WriteError(_),
                ) => {
                    tracing::warn!("Discord IPC connection was closed, reconnecting...");
                    self.is_connected = false;
                    self.client.connect()?;
                    self.is_connected = true;
                }
                Err(e) => {
                    tracing::error!("Failed to ping Discord IPC: {e}");
                    self.is_connected = false;
                    return Err(e.into());
                }
            }
        } else {
            tracing::info!("Connecting to Discord IPC");
            self.client.connect()?;
            self.is_connected = true;
        }
        Ok(())
    }

    fn set_activity(&mut self) -> aviutl2::AnyResult<()> {
        self.ensure_connected()?;
        tracing::info!("Updating Discord activity");
        self.client.set_activity(
            discord_rich_presence::activity::Activity::new()
                .state(tr("編集中..."))
                .timestamps(
                    discord_rich_presence::activity::Timestamps::new()
                        .start(self.started_at.unix_timestamp()),
                ),
        )?;
        Ok(())
    }

    fn clear_activity(&mut self) -> aviutl2::AnyResult<()> {
        self.ensure_connected()?;
        tracing::info!("Clearing Discord activity");
        self.client.clear_activity()?;
        Ok(())
    }

    fn save_config(&self) -> aviutl2::AnyResult<()> {
        tracing::debug!("Saving config: {:?}", self.config);
        std::fs::write(&*CONFIG_PATH, serde_json::to_string_pretty(&self.config)?)?;
        Ok(())
    }
}

#[aviutl2::generic::menus]
impl DiscordActivityAux2 {
    #[config(name = "[discord_activity.aux2] 有効/無効を切り替える")]
    fn toggle_enabled(&mut self, hwnd: aviutl2::Win32WindowHandle) {
        self.config.enabled = !self.config.enabled;
        if let Err(e) = self.save_config() {
            tracing::error!("Failed to save config: {e}");
        }
        aviutl2::tracing::info!("Toggled enabled");
        self.update_activity();
        native_dialog::DialogBuilder::message()
            .set_title(tr("discord_activity.aux2"))
            .set_text(if self.config.enabled {
                tr("Rich Presenceが有効になりました。")
            } else {
                tr("Rich Presenceが無効になりました。")
            })
            .set_owner(&unsafe {
                aviutl2::raw_window_handle::WindowHandle::borrow_raw(
                    aviutl2::raw_window_handle::RawWindowHandle::Win32(hwnd),
                )
            })
            .alert();
    }
}

aviutl2::register_generic_plugin!(DiscordActivityAux2);
