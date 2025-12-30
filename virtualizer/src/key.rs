#[cfg(not(feature = "std"))]
use alloc::collections::BTreeMap;
#[cfg(feature = "std")]
use std::collections::HashMap;

#[cfg(feature = "std")]
pub(crate) type KeySizeMap<K> = HashMap<K, u32>;
#[cfg(not(feature = "std"))]
pub(crate) type KeySizeMap<K> = BTreeMap<K, u32>;

#[cfg(feature = "std")]
#[doc(hidden)]
pub trait KeyCacheKey: core::hash::Hash + Eq {}
#[cfg(feature = "std")]
impl<K: core::hash::Hash + Eq> KeyCacheKey for K {}

#[cfg(not(feature = "std"))]
#[doc(hidden)]
pub trait KeyCacheKey: Ord {}
#[cfg(not(feature = "std"))]
impl<K: Ord> KeyCacheKey for K {}
