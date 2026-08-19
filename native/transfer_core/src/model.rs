use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferDirection {
    Incoming,
    Outgoing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferState {
    Preparing,
    Connecting,
    Pairing,
    WaitingForAcceptance,
    Transferring,
    Paused,
    Interrupted,
    Verifying,
    Completed,
    Failed,
    Cancelled,
}

impl TransferState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }

    pub fn is_active(self) -> bool {
        !matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    fn can_transition_to(self, next: Self) -> bool {
        use TransferState::*;
        match (self, next) {
            (state, Cancelled) if !state.is_terminal() => true,
            (Preparing, Connecting | Transferring | Failed) => true,
            (Connecting, Pairing | WaitingForAcceptance | Transferring | Interrupted | Failed) => {
                true
            }
            (Pairing, WaitingForAcceptance | Failed) => true,
            (WaitingForAcceptance, Transferring | Interrupted | Failed) => true,
            (Transferring, Paused | Interrupted | Verifying | Failed) => true,
            (Paused, Transferring | Interrupted | Failed) => true,
            (Interrupted | Failed, Preparing) => true,
            (Verifying, Completed | Failed) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferCommand {
    Pause,
    Resume,
    Cancel,
    Retry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferSnapshot {
    pub id: Uuid,
    pub peer_id: String,
    pub peer_name: String,
    pub direction: TransferDirection,
    pub state: TransferState,
    pub item_count: u32,
    pub total_bytes: u64,
    pub completed_bytes: u64,
    #[serde(default)]
    pub bytes_per_second: u64,
    pub created_unix_ms: i64,
    pub updated_unix_ms: i64,
    pub error: Option<String>,
}

impl TransferSnapshot {
    pub fn new_outgoing(
        peer_id: impl Into<String>,
        peer_name: impl Into<String>,
        item_count: u32,
        total_bytes: u64,
        now_unix_ms: i64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            peer_id: peer_id.into(),
            peer_name: peer_name.into(),
            direction: TransferDirection::Outgoing,
            state: TransferState::Preparing,
            item_count,
            total_bytes,
            completed_bytes: 0,
            bytes_per_second: 0,
            created_unix_ms: now_unix_ms,
            updated_unix_ms: now_unix_ms,
            error: None,
        }
    }

    pub fn new_incoming(
        id: Uuid,
        peer_id: impl Into<String>,
        peer_name: impl Into<String>,
        item_count: u32,
        total_bytes: u64,
        now_unix_ms: i64,
    ) -> Self {
        Self {
            id,
            peer_id: peer_id.into(),
            peer_name: peer_name.into(),
            direction: TransferDirection::Incoming,
            state: TransferState::WaitingForAcceptance,
            item_count,
            total_bytes,
            completed_bytes: 0,
            bytes_per_second: 0,
            created_unix_ms: now_unix_ms,
            updated_unix_ms: now_unix_ms,
            error: None,
        }
    }

    pub fn transition_to(&mut self, next: TransferState) -> Result<(), LifecycleError> {
        if !self.state.can_transition_to(next) {
            return Err(LifecycleError::InvalidTransition {
                from: self.state,
                to: next,
            });
        }
        self.state = next;
        Ok(())
    }

    pub fn apply_command(&mut self, command: TransferCommand) -> Result<(), LifecycleError> {
        let next = match command {
            TransferCommand::Pause => TransferState::Paused,
            TransferCommand::Resume => TransferState::Transferring,
            TransferCommand::Cancel => TransferState::Cancelled,
            TransferCommand::Retry => TransferState::Preparing,
        };
        self.transition_to(next)
    }

    #[cfg(test)]
    fn outgoing_for_test(total_bytes: u64) -> Self {
        let mut snapshot = Self::new_outgoing("peer-test", "测试设备", 1, total_bytes, 0);
        snapshot.id = Uuid::nil();
        snapshot
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedPeer {
    pub peer_id: String,
    pub display_name: String,
    pub certificate_fingerprint: [u8; 32],
    pub created_unix_ms: i64,
    pub last_seen_unix_ms: i64,
    pub auto_accept: bool,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LifecycleError {
    #[error("任务状态不能从 {from:?} 变为 {to:?}")]
    InvalidTransition {
        from: TransferState,
        to: TransferState,
    },
}

#[cfg(test)]
mod tests {
    use super::{TransferCommand, TransferSnapshot, TransferState};

    #[test]
    fn transfer_commands_follow_the_documented_lifecycle() {
        let mut transfer = TransferSnapshot::outgoing_for_test(42);
        transfer
            .transition_to(TransferState::Transferring)
            .expect("preparing can advance to transferring in restored jobs");

        transfer
            .apply_command(TransferCommand::Pause)
            .expect("active transfer can pause");
        assert_eq!(transfer.state, TransferState::Paused);

        transfer
            .apply_command(TransferCommand::Resume)
            .expect("paused transfer can resume");
        assert_eq!(transfer.state, TransferState::Transferring);

        transfer
            .transition_to(TransferState::Interrupted)
            .expect("network interruption is recoverable");
        transfer
            .apply_command(TransferCommand::Retry)
            .expect("interrupted transfer can retry");
        assert_eq!(transfer.state, TransferState::Preparing);

        transfer
            .apply_command(TransferCommand::Cancel)
            .expect("preparing transfer can be cancelled");
        assert_eq!(transfer.state, TransferState::Cancelled);
        assert!(transfer.apply_command(TransferCommand::Retry).is_err());
    }
}
