#[derive(Debug)]
pub enum TransformationError {}

pub type TransformationResult = Result<(), TransformationError>;
pub type PhaseResult = Result<(), Vec<TransformationError>>;
