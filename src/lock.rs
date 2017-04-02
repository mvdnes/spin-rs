pub trait Lock
{
    fn try_lock(&self) -> bool;
    fn lock(&self);
    fn unlock(&self);
}
