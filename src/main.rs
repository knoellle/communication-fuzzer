use std::{str::FromStr, sync::Arc, time::Duration};

use communication::client::{Communication, CyclerOutput};
use indicatif::{MultiProgress, ProgressBar};
use rand::{seq::SliceRandom, thread_rng};
use tokio::{spawn, sync::Semaphore, time::sleep};

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    console_subscriber::init();
    let client = Communication::new(Some("ws://10.1.24.23:1337".to_string()), true);
    let fields = loop {
        sleep(Duration::from_millis(100)).await;
        if let Some(fields) = client.get_output_fields().await {
            break fields;
        };
    };
    let fields: Vec<String> = fields
        .iter()
        .filter(|(cycler, _)| matches!(cycler.as_str(), "Control" | "VisionTop" | "VisionBottom"))
        .flat_map(|(cycler, paths)| paths.iter().map(move |path| format!("{cycler}.{path}")))
        .filter(|path| !path.ends_with("outputs") && !path.contains("image"))
        .collect();
    let multi = MultiProgress::new();
    let subscriptions = Arc::new(Semaphore::new(100));
    let mut rng = thread_rng();

    loop {
        let field = fields.choose(&mut rng).unwrap().clone();
        let task = multi.add(ProgressBar::new_spinner());
        task.set_message(field.clone());

        let client = client.clone();
        let subscriptions = subscriptions.clone();
        let permit = subscriptions.clone().acquire_owned().await.unwrap();
        spawn(async move {
            let (uuid, mut receiver) = client
                .subscribe_output(
                    CyclerOutput::from_str(&field).unwrap(),
                    communication::messages::Format::Textual,
                )
                .await;
            let mut count = 0;
            while let Some(message) = receiver.recv().await {
                count += 1;
                task.tick();
                task.set_message(format!(
                    "{} {count} {field}",
                    subscriptions.available_permits()
                ));
                if let communication::client::SubscriberMessage::SubscriptionFailure { .. } =
                    message
                {
                    task.println(format!("{field}, {message:#?}"));
                    break;
                }
                if count > 100 + 100 % field.len() {
                    break;
                }
            }
            drop(receiver);
            client.unsubscribe_output(uuid).await;
            sleep(Duration::from_millis(200)).await;
            drop(permit);
        });
    }
}
