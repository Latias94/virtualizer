#[cfg(feature = "std")]
pub trait VirtualizerKey: core::hash::Hash + Eq {}
#[cfg(feature = "std")]
impl<T: core::hash::Hash + Eq> VirtualizerKey for T {}

#[cfg(not(feature = "std"))]
pub trait VirtualizerKey: Ord {}
#[cfg(not(feature = "std"))]
impl<T: Ord> VirtualizerKey for T {}
