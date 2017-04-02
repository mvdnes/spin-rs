
/// Public abstraction for a lock implementation.
pub trait Lock
{
	/// true when a lock is acquired successfully
	/// false otherwise
    fn try_lock(&self) -> bool;
    
    /// obtain a lock
    fn lock(&self);
    
    /// release a lock
    fn unlock(&self);
}
