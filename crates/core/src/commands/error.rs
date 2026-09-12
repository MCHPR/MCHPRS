use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("Coordinates are outside the supported range")]
    CoordinateOutOfRange,
    #[error("Destination is outside plot bounds")]
    DestinationOutsidePlot,
    #[error("No block in sight")]
    NoBlockInSight,
    #[error("Plot ownership required")]
    PlotOwnershipRequired,
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
    #[error("Internal error: Argument '{name}' not found (command definition bug)")]
    MissingArgument { name: String },
    #[error("Internal error: Argument '{name}' is {found}, expected {expected} (command definition bug)")]
    WrongArgumentType {
        name: String,
        expected: &'static str,
        found: String,
    },
    #[error("Internal error: Player index {index} is invalid (state management bug)")]
    InvalidPlayerIndex { index: usize },
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
}

pub type CommandResult<T> = Result<T, CommandError>;
