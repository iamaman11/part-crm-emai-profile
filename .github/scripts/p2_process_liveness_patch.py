from pathlib import Path


def replace_once(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one replacement, found {count}")
    path.write_text(text.replace(old, new), encoding="utf-8")


camouhost = Path("apps/profile-bridge/src/camouhost_process.rs")
replace_once(
    camouhost,
    """    fn request_graceful_close(&mut self, session_id: &SessionId) -> Result<(), BridgePortError> {
        let state = self
            .shared
            .lock()
            .map_err(|_| BridgePortError::Unavailable)?;
        if state.active_session.as_ref() != Some(session_id) {
            return Err(BridgePortError::InvalidResponse);
        }
        Ok(())
    }
""",
    """    fn is_running(&mut self, session_id: &SessionId) -> Result<bool, BridgePortError> {
        let mut state = self
            .shared
            .lock()
            .map_err(|_| BridgePortError::Unavailable)?;
        if state.active_session.as_ref() != Some(session_id) {
            return Err(BridgePortError::InvalidResponse);
        }
        let child = state
            .child
            .as_mut()
            .ok_or(BridgePortError::InvalidResponse)?;
        child
            .try_wait()
            .map(|status| status.is_none())
            .map_err(|_| BridgePortError::Unavailable)
    }

    fn request_graceful_close(&mut self, session_id: &SessionId) -> Result<(), BridgePortError> {
        let state = self
            .shared
            .lock()
            .map_err(|_| BridgePortError::Unavailable)?;
        if state.active_session.as_ref() != Some(session_id) {
            return Err(BridgePortError::InvalidResponse);
        }
        Ok(())
    }
""",
    "managed Camouhost is_running",
)

operator = Path("apps/profile-bridge/src/operator_flow.rs")
replace_once(
    operator,
    """    pub fn heartbeat(&mut self, now: UnixMillis) -> Result<(), OperatorFlowError>
    where
        C: ProfileCoordinatorRuntimePort,
    {
        let lease = self
            .active
            .as_ref()
            .map(|session| session.lease.clone())
            .ok_or(OperatorFlowError::Stage(
                OperatorFailureStage::CoordinatorHeartbeat,
            ))?;
        if self.coordinator.heartbeat_lease(&lease).is_ok() {
            return Ok(());
        }

        let Some(mut session) = self.active.take() else {
            return Err(OperatorFlowError::Stage(
                OperatorFailureStage::CoordinatorHeartbeat,
            ));
        };
        let process_failed = self
            .process
            .force_terminate(session.lease.session_id())
            .is_err();
        if session.local_record.observe_crash(now).is_err() {
            return Err(self.finish_failed_session(
                OperatorFailureStage::CoordinatorHeartbeat,
                session,
                process_failed,
            ));
        }
        let cleanup = self.cleanup_active_session(&mut session, process_failed);
        self.record_terminal(&session.lease, &session.local_record, cleanup);
        if cleanup.any() {
            self.cleanup_blocked = true;
        }
        Err(OperatorFlowError::Terminal {
            stage: OperatorFailureStage::CoordinatorHeartbeat,
            local_state: session.local_record.state(),
            cleanup,
        })
    }
""",
    """    pub fn heartbeat(&mut self, now: UnixMillis) -> Result<(), OperatorFlowError>
    where
        C: ProfileCoordinatorRuntimePort,
    {
        let lease = self
            .active
            .as_ref()
            .map(|session| session.lease.clone())
            .ok_or(OperatorFlowError::Stage(
                OperatorFailureStage::CoordinatorHeartbeat,
            ))?;
        if !matches!(self.process.is_running(lease.session_id()), Ok(true)) {
            return Err(self.fail_active_runtime(OperatorFailureStage::RuntimeAbort, now));
        }
        if self.coordinator.heartbeat_lease(&lease).is_ok() {
            return Ok(());
        }
        Err(self.fail_active_runtime(OperatorFailureStage::CoordinatorHeartbeat, now))
    }
""",
    "operator heartbeat liveness",
)
replace_once(
    operator,
    """    #[must_use]
    pub const fn process(&self) -> &P {
        &self.process
    }

    fn fail_before_local_use(
""",
    """    #[must_use]
    pub const fn process(&self) -> &P {
        &self.process
    }

    #[cfg(test)]
    fn process_mut(&mut self) -> &mut P {
        &mut self.process
    }

    fn fail_before_local_use(
""",
    "test process mutable accessor",
)
replace_once(
    operator,
    """    fn cleanup_after_local_use(
""",
    """    fn fail_active_runtime(
        &mut self,
        stage: OperatorFailureStage,
        now: UnixMillis,
    ) -> OperatorFlowError {
        let Some(mut session) = self.active.take() else {
            return OperatorFlowError::Stage(stage);
        };
        let process_failed = self
            .process
            .force_terminate(session.lease.session_id())
            .is_err();
        let _ = session.local_record.observe_crash(now);
        self.finish_failed_session(stage, session, process_failed)
    }

    fn cleanup_after_local_use(
""",
    "operator active runtime failure helper",
)
healthy = """    #[test]
    fn healthy_heartbeat_preserves_active_runtime_ownership()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let mut operator = operator(&fixture, FakeCamouhost::default())?;
        operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(10))?;
        operator.heartbeat(UnixMillis::new(11))?;
        assert_eq!(operator.coordinator().heartbeats, 1);
        assert_eq!(
            operator.active_session_id(),
            Some(fixture.lease.session_id())
        );
        assert_eq!(
            operator.process().actions(),
            [ProcessAction::Spawn(fixture.lease.session_id().clone())]
        );
        Ok(())
    }

"""
dead = """    #[test]
    fn dead_runtime_is_fenced_before_coordinator_heartbeat()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let mut operator = operator(&fixture, FakeCamouhost::default())?;
        operator.open(&fixture.claim_uri, &fixture.root, UnixMillis::new(10))?;
        operator
            .process_mut()
            .simulate_exit(fixture.lease.session_id())?;
        assert_eq!(
            operator.heartbeat(UnixMillis::new(11)),
            Err(OperatorFlowError::Terminal {
                stage: OperatorFailureStage::RuntimeAbort,
                local_state: LocalGenerationState::RecoveryRequired,
                cleanup: super::CleanupFailures::none(),
            })
        );
        assert_eq!(operator.coordinator().heartbeats, 0);
        assert_eq!(operator.coordinator().closed, 1);
        assert_eq!(operator.active_session_id(), None);
        assert_eq!(
            operator.last_terminal().map(|value| value.local_state()),
            Some(LocalGenerationState::RecoveryRequired)
        );
        assert_eq!(
            operator.process().actions(),
            [
                ProcessAction::Spawn(fixture.lease.session_id().clone()),
                ProcessAction::ForceTerminate(fixture.lease.session_id().clone()),
            ]
        );
        Ok(())
    }

"""
replace_once(operator, healthy, healthy + dead, "dead runtime acceptance")
