use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("No block in sight")]
    NoBlockInSight,
    #[error("Permission denied: {permission}")]
    PermissionDenied { permission: String },
    #[error("Plot ownership required")]
    PlotOwnershipRequired,
    #[error("This command can only be executed by players")]
    PlayerOnly,
    #[error("Make a region selection first.")]
    NoSelection,
    #[error("{position} position is outside plot bounds")]
    SelectionOutOfBounds { position: String },
    #[error("Your clipboard is empty. Use //copy first.")]
    EmptyClipboard,
    #[error("There is nothing left to undo.")]
    NoUndoHistory,
    #[error("There is nothing left to redo.")]
    NoRedoHistory,
    #[error("Undo is from a different plot")]
    UndoFromDifferentPlot,
    #[error("Redo is from a different plot")]
    RedoFromDifferentPlot,
    #[error("{0}")]
    Message(String),
}

#[derive(Debug, Error)]
pub enum InternalError {
    #[error(
        "Internal error: Argument '{name}' not found in ArgumentSet (command registration bug)"
    )]
    MissingArgument { name: String },
    #[error("Internal error: Argument '{name}' has wrong type, expected {expected} (command registration bug)")]
    WrongArgumentType { name: String, expected: String },
    #[error("Internal error: Player index {index} is invalid (state management bug)")]
    InvalidPlayerIndex { index: usize },
    #[error("Internal error (bug): {message}")]
    Message { message: String },
}

#[derive(Debug, Error)]
pub enum CommandError {
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error(transparent)]
    Internal(#[from] InternalError),
}

impl CommandError {
    pub fn runtime(message: impl Into<String>) -> Self {
        CommandError::Runtime(RuntimeError::Message(message.into()))
    }

    pub fn internal(message: impl Into<String>) -> Self {
        CommandError::Internal(InternalError::Message {
            message: message.into(),
        })
    }
}

pub type CommandResult<T> = Result<T, CommandError>;

pub(crate) trait UnwrapRuntimeError<T> {
    fn unwrap_runtime(self) -> Result<T, InternalError>;
}

impl<T> UnwrapRuntimeError<T> for CommandResult<T> {
    fn unwrap_runtime(self) -> Result<T, InternalError> {
        self.map_err(|err| match err {
            CommandError::Runtime(err) => InternalError::Message {
                message: format!("Runtime Error: {}", err),
            },
            CommandError::Internal(err) => err,
        })
    }
}
