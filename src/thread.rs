use aviutl2::{config::translate as tr, tracing};
use discord_rich_presence::DiscordIpc;

pub(crate) enum ThreadMessage {
    SetStartedAt(time::OffsetDateTime),
    ClearActivity,
    SetActivity,
    Shutdown,
}

pub(crate) struct DiscordWorker {
    tx: std::sync::mpsc::Sender<ThreadMessage>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl DiscordWorker {
    pub(crate) fn new(client_id: &str) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let client_id = client_id.to_string();
        let handle = std::thread::spawn(move || {
            let mut is_connected = false;
            let mut client = discord_rich_presence::DiscordIpcClient::new(&client_id);
            let mut started_at = time::OffsetDateTime::now_utc();

            while let Ok(message) = rx.recv() {
                let result = match message {
                    ThreadMessage::SetStartedAt(started) => {
                        started_at = started;
                        Ok(())
                    }
                    ThreadMessage::SetActivity => {
                        set_activity(&mut client, &mut is_connected, started_at)
                    }
                    ThreadMessage::ClearActivity => clear_activity(&mut client, &mut is_connected),
                    ThreadMessage::Shutdown => {
                        if let Err(e) = disconnect(&mut client, &mut is_connected) {
                            tracing::error!("Failed to disconnect from Discord IPC: {e}");
                        }
                        break;
                    }
                };

                if let Err(e) = result {
                    tracing::error!("Discord worker failed to handle message: {e}");
                }
            }

            if let Err(e) = disconnect(&mut client, &mut is_connected) {
                tracing::error!("Failed to disconnect from Discord IPC on worker shutdown: {e}");
            }
        });

        Self {
            tx,
            handle: Some(handle),
        }
    }

    pub(crate) fn send(&self, message: ThreadMessage) {
        if let Err(e) = self.tx.send(message) {
            tracing::error!("Failed to send message to Discord worker: {e}");
        }
    }
}

impl Drop for DiscordWorker {
    fn drop(&mut self) {
        if let Err(e) = self.tx.send(ThreadMessage::Shutdown) {
            tracing::debug!("Failed to send shutdown message to Discord worker: {e}");
        }
        if let Some(handle) = self.handle.take()
            && handle.join().is_err()
        {
            tracing::error!("Discord worker thread panicked");
        }
    }
}

fn ensure_connected(
    client: &mut discord_rich_presence::DiscordIpcClient,
    is_connected: &mut bool,
) -> aviutl2::AnyResult<()> {
    if *is_connected {
        tracing::info!("Pinging Discord IPC");
        match client.send(serde_json::json!({}), 4) {
            Ok(_) => {
                tracing::info!("Discord IPC connection is healthy");
            }
            Err(
                discord_rich_presence::error::Error::NotConnected
                | discord_rich_presence::error::Error::WriteError(_),
            ) => {
                tracing::warn!("Discord IPC connection was closed, reconnecting...");
                *is_connected = false;
                client.connect()?;
                *is_connected = true;
            }
            Err(e) => {
                tracing::error!("Failed to ping Discord IPC: {e}");
                *is_connected = false;
                return Err(e.into());
            }
        }
    } else {
        tracing::info!("Connecting to Discord IPC");
        client.connect()?;
        *is_connected = true;
    }
    Ok(())
}

fn disconnect(
    client: &mut discord_rich_presence::DiscordIpcClient,
    is_connected: &mut bool,
) -> aviutl2::AnyResult<()> {
    if *is_connected {
        tracing::info!("Disconnecting from Discord IPC");
        clear_activity(client, is_connected)?;
        client.close()?;
        *is_connected = false;
    }
    Ok(())
}

fn set_activity(
    client: &mut discord_rich_presence::DiscordIpcClient,
    is_connected: &mut bool,
    started_at: time::OffsetDateTime,
) -> aviutl2::AnyResult<()> {
    ensure_connected(client, is_connected)?;
    tracing::info!("Updating Discord activity");
    client.set_activity(
        discord_rich_presence::activity::Activity::new()
            .state(tr("編集中..."))
            .timestamps(
                discord_rich_presence::activity::Timestamps::new()
                    .start(started_at.unix_timestamp()),
            ),
    )?;
    Ok(())
}

fn clear_activity(
    client: &mut discord_rich_presence::DiscordIpcClient,
    is_connected: &mut bool,
) -> aviutl2::AnyResult<()> {
    ensure_connected(client, is_connected)?;
    tracing::info!("Clearing Discord activity");
    client.clear_activity()?;
    Ok(())
}
