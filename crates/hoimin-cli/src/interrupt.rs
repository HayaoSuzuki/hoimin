pub(crate) struct InterruptMonitor {
    first: tokio::sync::oneshot::Receiver<Result<(), String>>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl InterruptMonitor {
    pub(crate) fn spawn() -> Self {
        let (raw_tx, raw_rx) = tokio::sync::mpsc::unbounded_channel();
        let producer = tokio::spawn(async move {
            loop {
                let signal = tokio::signal::ctrl_c()
                    .await
                    .map_err(|error| format!("install Ctrl+C handler: {error}"));
                let failed = signal.is_err();
                if raw_tx.send(signal).is_err() || failed {
                    break;
                }
            }
        });
        spawn_monitor(raw_rx, |code| std::process::exit(code), Some(producer))
    }

    pub(crate) async fn first(&mut self) -> Result<(), String> {
        (&mut self.first)
            .await
            .unwrap_or_else(|_| Err("install Ctrl+C handler: monitor stopped".to_owned()))
    }
}

impl Drop for InterruptMonitor {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

fn spawn_monitor<Terminate>(
    mut raw_signals: tokio::sync::mpsc::UnboundedReceiver<Result<(), String>>,
    terminate: Terminate,
    producer_task: Option<tokio::task::JoinHandle<()>>,
) -> InterruptMonitor
where
    Terminate: FnOnce(i32) + Send + 'static,
{
    let (first_tx, first) = tokio::sync::oneshot::channel();
    let monitor_task = tokio::spawn(async move {
        let first = raw_signals
            .recv()
            .await
            .unwrap_or_else(|| Err("install Ctrl+C handler: signal producer stopped".to_owned()));
        let failed = first.is_err();
        let _ = first_tx.send(first);
        if failed {
            return;
        }
        if matches!(raw_signals.recv().await, Some(Ok(()))) {
            terminate(130);
        }
    });
    let mut tasks = vec![monitor_task];
    if let Some(producer_task) = producer_task {
        tasks.push(producer_task);
    }
    InterruptMonitor { first, tasks }
}

#[cfg(test)]
fn spawn_test_monitor<Terminate>(
    raw_signals: tokio::sync::mpsc::UnboundedReceiver<Result<(), String>>,
    terminate: Terminate,
) -> InterruptMonitor
where
    Terminate: FnOnce(i32) + Send + 'static,
{
    spawn_monitor(raw_signals, terminate, None)
}

#[cfg(test)]
mod tests {
    use super::spawn_test_monitor;

    #[tokio::test]
    async fn first_signal_is_forwarded_without_forcing_exit() {
        let (signal_tx, signal_rx) = tokio::sync::mpsc::unbounded_channel();
        let (forced_tx, mut forced_rx) = tokio::sync::oneshot::channel();
        let mut monitor = spawn_test_monitor(signal_rx, move |code| {
            let _ = forced_tx.send(code);
        });

        signal_tx.send(Ok(())).unwrap();
        assert_eq!(monitor.first().await, Ok(()));
        assert!(forced_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn second_signal_forces_130_without_waiting_for_first_consumer() {
        let (signal_tx, signal_rx) = tokio::sync::mpsc::unbounded_channel();
        let (forced_tx, forced_rx) = tokio::sync::oneshot::channel();
        let _monitor = spawn_test_monitor(signal_rx, move |code| {
            let _ = forced_tx.send(code);
        });

        signal_tx.send(Ok(())).unwrap();
        signal_tx.send(Ok(())).unwrap();
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(1), forced_rx)
                .await
                .expect("second signal must not depend on scheduler polling")
                .unwrap(),
            130
        );
    }

    #[tokio::test]
    async fn first_handler_failure_is_reported_and_never_escalates() {
        let (signal_tx, signal_rx) = tokio::sync::mpsc::unbounded_channel();
        let (forced_tx, mut forced_rx) = tokio::sync::oneshot::channel();
        let mut monitor = spawn_test_monitor(signal_rx, move |code| {
            let _ = forced_tx.send(code);
        });

        signal_tx
            .send(Err("install Ctrl+C handler: fixture".to_owned()))
            .unwrap();
        let error = monitor.first().await.unwrap_err();
        assert!(error.contains("install Ctrl+C handler"), "{error}");
        assert!(error.contains("fixture"), "{error}");
        assert!(forced_rx.try_recv().is_err());
    }
}
