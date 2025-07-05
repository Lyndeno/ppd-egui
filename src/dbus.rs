use crate::state::PpdValue;
use futures_lite::stream::StreamExt;
use ppd::PpdProxy;
use tokio::sync::mpsc::{Receiver, Sender};
use zbus::Connection;

pub async fn setup_dbus(
    sender: Sender<PpdValue>,
    mut receiver: Receiver<PpdValue>,
    cancellation_token: tokio_util::sync::CancellationToken,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let conn = Connection::system().await?;
    let proxy = PpdProxy::new(&conn).await?;

    let rp = if let Some(PpdValue::Context(c)) = receiver.recv().await {
        Some(c)
    } else {
        None
    };

    // Set up signal listeners
    let mut profile_signal = proxy.receive_active_profile_changed().await;
    let mut battery_aware_signal = proxy.receive_battery_aware_changed().await;

    // Task to handle profile changes
    let sender_clone = sender.clone();
    let rp_clone = rp.clone();
    let profile_task = crate::runtime().spawn(async move {
        while let Some(signal) = profile_signal.next().await {
            if let Ok(profile) = signal.get().await {
                if sender_clone
                    .send(PpdValue::Profile(profile.into()))
                    .await
                    .is_err()
                {
                    break;
                } else if let Some(ctx) = &rp_clone {
                    ctx.request_repaint();
                }
            }
        }
    });

    // Task to handle battery aware changes
    let sender_clone = sender.clone();
    let rp_clone = rp.clone();
    let battery_aware_task = crate::runtime().spawn(async move {
        while let Some(signal) = battery_aware_signal.next().await {
            if let Ok(ba) = signal.get().await {
                if sender_clone.send(PpdValue::BatteryAware(ba)).await.is_err() {
                    break;
                } else if let Some(ctx) = &rp_clone {
                    ctx.request_repaint();
                }
            }
        }
    });

    // Task to handle incoming requests
    let setter_task = crate::runtime().spawn(async move {
        while let Some(value) = receiver.recv().await {
            match value {
                PpdValue::Profile(p) => {
                    if proxy.set_active_profile(p.to_string()).await.is_err() {
                        break;
                    }
                }
                PpdValue::BatteryAware(ba) => {
                    if proxy.set_battery_aware(ba).await.is_err() {
                        break;
                    }
                }
                PpdValue::Context(_) => {}
            }
        }
    });

    // Wait for tasks to complete or be cancelled
    tokio::select! {
        () = cancellation_token.cancelled() => {},
        _ = profile_task => {},
        _ = battery_aware_task => {},
        _ = setter_task => {},
    };

    Ok(())
}
