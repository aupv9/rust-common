use common_error::Result;

pub trait Validate {
    fn validate(&self) -> Result<()>;
}
