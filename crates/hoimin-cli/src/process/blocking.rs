use hoimin_core::{EffectFailed, EffectId};

pub(super) struct BlockingOwner<T: Send + 'static> {
    value: Option<T>,
}

impl<T: Send + 'static> BlockingOwner<T> {
    pub(super) fn new(value: T) -> Self {
        Self { value: Some(value) }
    }

    pub(super) fn value_mut(&mut self, id: EffectId) -> Result<&mut T, EffectFailed> {
        self.value.as_mut().ok_or_else(|| missing_owner(id))
    }

    pub(super) async fn run<R: Send + 'static>(
        &mut self,
        id: EffectId,
        operation: impl FnOnce(&mut T) -> R + Send + 'static,
    ) -> Result<R, EffectFailed> {
        let (mut owner, result) = self
            .dispatch(id, operation)?
            .await
            .map_err(|error| join_failure(id, &error))?;
        self.value = owner.value.take();
        Ok(result)
    }

    fn dispatch<R: Send + 'static>(
        &mut self,
        id: EffectId,
        operation: impl FnOnce(&mut T) -> R + Send + 'static,
    ) -> Result<tokio::task::JoinHandle<(Self, R)>, EffectFailed> {
        let mut value = self.value.take().ok_or_else(|| missing_owner(id))?;
        Ok(tokio::task::spawn_blocking(move || {
            let result = operation(&mut value);
            (Self::new(value), result)
        }))
    }

    pub(super) async fn finish(&mut self, id: EffectId) -> Result<(), EffectFailed> {
        let Some(value) = self.value.take() else {
            return Ok(());
        };
        tokio::task::spawn_blocking(move || drop(value))
            .await
            .map_err(|error| join_failure(id, &error))
    }
}

impl<T: Send + 'static> Drop for BlockingOwner<T> {
    fn drop(&mut self) {
        if let Some(value) = self.value.take() {
            tokio::task::spawn_blocking(move || drop(value));
        }
    }
}

fn missing_owner(id: EffectId) -> EffectFailed {
    EffectFailed::other(
        id,
        "process.resource.owner",
        "resource owner is unavailable",
    )
}

fn join_failure(id: EffectId, error: &tokio::task::JoinError) -> EffectFailed {
    EffectFailed::other(
        id,
        "process.resource.join",
        format!("owned resource operation failed: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    use hoimin_core::EffectId;
    use tokio::sync::Notify;

    use super::BlockingOwner;

    #[tokio::test(flavor = "current_thread")]
    async fn resource_wait_allows_dispatcher_progress() {
        let mut owner = BlockingOwner::new(0);
        let entered = Arc::new(Notify::new());
        let operation_entered = Arc::clone(&entered);
        let (release, released) = mpsc::channel();
        let operation = owner.run(EffectId(17), move |value| {
            operation_entered.notify_one();
            let progressed = released.recv_timeout(Duration::from_secs(2)).is_ok();
            *value += 1;
            progressed
        });
        let dispatcher = async move {
            entered.notified().await;
            tokio::task::yield_now().await;
            let _ = release.send(());
        };
        let (progressed, ()) = tokio::join!(operation, dispatcher);
        assert!(progressed.unwrap(), "resource wait blocked the dispatcher");
        assert_eq!(owner.value, Some(1));
    }

    struct DropThread(tokio::sync::oneshot::Sender<std::thread::ThreadId>);

    impl Drop for DropThread {
        fn drop(&mut self) {
            let (replacement, _) = tokio::sync::oneshot::channel();
            let sender = std::mem::replace(&mut self.0, replacement);
            let _ = sender.send(std::thread::current().id());
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancelled_wait_keeps_owner_until_operation_returns() {
        let dispatcher_thread = std::thread::current().id();
        let (dropped, mut drop_observation) = tokio::sync::oneshot::channel();
        let mut owner = BlockingOwner::new(DropThread(dropped));
        let (entered, entry) = tokio::sync::oneshot::channel();
        let (release, released) = mpsc::channel();
        let operation = owner.run(EffectId(17), move |_| {
            entered.send(()).unwrap();
            released.recv_timeout(Duration::from_secs(2)).unwrap();
        });
        let mut operation = Box::pin(operation);
        tokio::select! {
            result = &mut operation => panic!("operation finished before release: {result:?}"),
            () = async { entry.await.unwrap(); } => {}
        }
        drop(operation);
        drop(owner);
        assert!(matches!(
            drop_observation.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ));
        release.send(()).unwrap();
        let drop_thread = tokio::time::timeout(Duration::from_secs(2), drop_observation)
            .await
            .unwrap()
            .unwrap();
        assert_ne!(drop_thread, dispatcher_thread);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn completed_unconsumed_operation_destroys_owner_off_dispatcher() {
        let (dropped, observation) = tokio::sync::oneshot::channel();
        let mut owner = BlockingOwner::new(DropThread(dropped));
        let task = owner.dispatch(EffectId(17), |_| ()).unwrap();
        while !task.is_finished() {
            tokio::task::yield_now().await;
        }
        drop(task);
        drop(owner);
        assert_ne!(observation.await.unwrap(), std::thread::current().id());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn idle_owner_destruction_runs_off_dispatcher() {
        let dispatcher_thread = std::thread::current().id();
        for explicit_finish in [false, true] {
            let (dropped, observation) = tokio::sync::oneshot::channel();
            let mut owner = BlockingOwner::new(DropThread(dropped));
            if explicit_finish {
                owner.finish(EffectId(17)).await.unwrap();
            }
            drop(owner);
            assert_ne!(observation.await.unwrap(), dispatcher_thread);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn panic_preserves_effect_identity_and_drops_only_once() {
        let (dropped, observation) = tokio::sync::oneshot::channel();
        let mut owner = BlockingOwner::new(DropThread(dropped));
        let error = owner
            .run(EffectId(17), |_| panic!("resource panic"))
            .await
            .unwrap_err();
        assert_eq!(error.id, EffectId(17));
        assert_eq!(error.failure.code(), "process.resource.join");
        assert_ne!(observation.await.unwrap(), std::thread::current().id());
        let unavailable = owner.run(EffectId(18), |_| ()).await.unwrap_err();
        assert_eq!(unavailable.id, EffectId(18));
        assert_eq!(unavailable.failure.code(), "process.resource.owner");
    }
}
