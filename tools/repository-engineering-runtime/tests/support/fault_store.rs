use repository_engineering_runtime::adapters::checkpoint_fs::{CheckpointFault, FaultInjector};

#[derive(Debug, Default)]
pub struct FailOnce {
    point: Option<CheckpointFault>,
}

impl FailOnce {
    pub fn at(point: CheckpointFault) -> Self {
        Self { point: Some(point) }
    }
}

impl FaultInjector for FailOnce {
    fn should_fail(&mut self, point: CheckpointFault) -> bool {
        if self.point == Some(point) {
            self.point = None;
            true
        } else {
            false
        }
    }
}
