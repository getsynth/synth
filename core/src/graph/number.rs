#![allow(clippy::assertions_on_result_states)]
use super::prelude::*;

use rand::distributions::uniform::SampleRange;

use num::{CheckedAdd, One, Zero};

use std::ops::{Bound, RangeBounds};

/// A custom extension of all the range types that uses
/// [`Bound`](std::ops::Bound) on the left and on the right.
#[derive(Clone)]
struct AnyRange<N> {
    low: Bound<N>,
    high: Bound<N>,
}

macro_rules! any_range_int_impl {
    { $target:ty } => {
        impl AnyRange<$target> {
            fn is_empty(&self) -> bool {
                match (&self.low, &self.high) {
                    (Bound::Excluded(low), Bound::Included(high))
                        | (Bound::Included(low), Bound::Excluded(high)) => low >= high,
                    (Bound::Included(low), Bound::Included(high)) => low > high,
                    (Bound::Excluded(low), Bound::Excluded(high)) => *low + 1 >= *high,
                    _ => false
                }
            }
        }

        impl SampleRange<$target> for AnyRange<$target> {
            fn sample_single<R: rand::RngCore + ?Sized>(self, rng: &mut R) -> $target {
                let low = match self.low {
                    Bound::Unbounded => panic!("cannot sample {} range unbounded on the left", stringify!($target)),
                    Bound::Included(low) => low,
                    Bound::Excluded(low) => low + 1
                };

                match self.high {
                    Bound::Excluded(high) => rng.gen_range(low..high),
                    Bound::Included(high) => rng.gen_range(low..=high),
                    Bound::Unbounded => panic!("cannot sample {} range unbounded on the right", stringify!($target))
                }
            }

            fn is_empty(&self) -> bool {
                Self::is_empty(self)
            }
        }
    }
}

any_range_int_impl! { u8 }
any_range_int_impl! { u16 }
any_range_int_impl! { u32 }
any_range_int_impl! { u64 }

macro_rules! any_range_float_impl {
    { $target:ty } => {
        impl AnyRange<$target> {
            fn is_not_empty(&self) -> bool {
                match (&self.low, &self.high) {
                    (Bound::Excluded(low), Bound::Included(high))
                        | (Bound::Included(low), Bound::Excluded(high)) => low < high,
                    (Bound::Included(low), Bound::Included(high)) => low <= high,
                    _ => true
                }
            }
        }

        impl SampleRange<$target> for AnyRange<$target> {
            fn sample_single<R: rand::RngCore + ?Sized>(self, rng: &mut R) -> $target {
                let low = match self.low {
                    Bound::Excluded(_)
                    | Bound::Unbounded => panic!("cannot sample {} range unbounded or open on the left", stringify!($target)),
                    Bound::Included(low) => low,
                };

                let (high, include_high) = match self.high {
                    Bound::Unbounded => panic!("cannot sample {} range unbounded on the right", stringify!($target)),
                    Bound::Included(high) => (high, true),
                    Bound::Excluded(high) => (high, false)
                };

                if include_high {
                    rng.gen_range(low..=high)
                } else {
                    rng.gen_range(low..high)
                }
            }

            fn is_empty(&self) -> bool {
                // This is a double-negative because of NaNs.
                !Self::is_not_empty(self)
            }
        }
    }
}

any_range_float_impl! { f32 }
any_range_float_impl! { f64 }

impl<N: std::fmt::Display> AnyRange<N> {
    fn display_bound(bound: &Bound<N>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match bound {
            Bound::Included(n) => write!(f, "{n} (inclusive)"),
            Bound::Excluded(n) => write!(f, "{n} (exclusive)"),
            Bound::Unbounded => write!(f, "unbounded"),
        }
    }
}

impl<N> std::fmt::Display for AnyRange<N>
where
    N: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "low=")?;
        Self::display_bound(&self.low, f)?;
        write!(f, " high=")?;
        Self::display_bound(&self.high, f)
    }
}

impl<N: Clone> AnyRange<N> {
    fn from_range_bounds<R: RangeBounds<N>>(r: &R) -> Self {
        Self {
            low: r.start_bound().cloned(),
            high: r.end_bound().cloned(),
        }
    }
}

pub struct StandardIntRangeStep<R, L> {
    range: AnyRange<R>,
    step: R,
    low: L,
}

impl<R, L> std::fmt::Display for StandardIntRangeStep<R, L>
where
    R: std::fmt::Display,
    L: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.range.fmt(f)?;
        write!(f, " step={}", self.step)
    }
}

fn into_finite_bound<N>(n: N, inclusive: bool) -> Bound<N> {
    if inclusive {
        Bound::Included(n)
    } else {
        Bound::Excluded(n)
    }
}

/// Macroed implementation of [`Distribution`](rand::distributions::Distribution) for integer
/// primitive ranges.
///
/// The default behavior is to cast the lower and upper bounds of the range (instances
/// of `$target`) to `$larger`, where `high - low` is computed and truncated down to `$unsigned`.
///
/// In particular when `$target == $larger == $unsigned` is an unsigned integer type these are
/// no-ops.
macro_rules! standard_int_range_step_impl {
    { $target:ty, $larger:ty, $unsigned:ty } => {
        impl StandardIntRangeStep<$unsigned, $larger> {
            pub fn try_from_range(range_step: RangeStep<$target>) -> anyhow::Result<Self> {
                let low = range_step.low.unwrap_or(<$target>::MIN);
                let high = range_step.high.unwrap_or(<$target>::MAX);
                if low > high {
                    return Err(anyhow!("integer range cannot have 'low'={} > 'high'={}", low, high))
                }

                let delta = ((high as $larger) - (low as $larger)) as $unsigned;

                let step_unchecked = range_step.step.unwrap_or(1);

                #[allow(unused_comparisons)]
                if step_unchecked == 0 {
                    return Err(anyhow!("integer range 'step'=0 is invalid, use the constant generator instead"))
                } else if step_unchecked < 0 {
                    return Err(anyhow!("integer range 'step'={} is invalid, use a positive value instead", step_unchecked))
                }

                let step = step_unchecked as $unsigned;

                let include_low = range_step.include_low;
                let include_high = range_step.include_high;

                let delta_include_high = if delta % step == 0 {
                    include_high
                } else {
                    true
                };

                let delta_high = into_finite_bound(delta / step, delta_include_high);
                let delta_low = into_finite_bound(0, include_low);
                let range = AnyRange::from_range_bounds(&(delta_low, delta_high));

                if range.is_empty() {
                    let original = AnyRange::from_range_bounds(&range_step);
                    Err(anyhow!("{} range with {} step={} is empty", stringify!($target), original, step))
                } else {
                    Ok(Self { range, low: low as $larger, step })
                }
            }
        }

        impl Distribution<$target> for StandardIntRangeStep<$unsigned, $larger> {
            fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> $target {
                let num_steps = rng.gen_range(self.range.clone());
                let delta = num_steps * self.step;
                (delta as $larger + self.low) as $target
            }
        }
    }
}

standard_int_range_step_impl! { i8, i16, u8}
standard_int_range_step_impl! { i16, i32, u16}
standard_int_range_step_impl! { i32, i64, u32 }
standard_int_range_step_impl! { u32, u32, u32 }
standard_int_range_step_impl! { i64, i128, u64 }
standard_int_range_step_impl! { u64, u64, u64 }

pub struct StandardFloatRangeStep<N> {
    range: AnyRange<N>,
    low: Option<N>,
    step: Option<N>,
}

macro_rules! standard_float_range_step_impl {
    { $target:ty } => {
        impl StandardFloatRangeStep<$target> {
            pub fn try_from_range(range_step: RangeStep<$target>) -> anyhow::Result<Self> {
                let step = range_step.step;
                if Some(true) == step.as_ref().map(|step| *step <= 0.) {
                    return Err(anyhow!("{} range with step={} is invalid, use positive values instead", stringify!($target), step.unwrap()));
                }

                let mut low = range_step.low.unwrap_or(0.);
                if !range_step.include_low {
                    let step = step.as_ref()
                        .ok_or_else(|| anyhow!("{} range with include_low=false must set the 'step' parameter", stringify!($target)))?;
                    low += *step;
                }
                let low_bound = into_finite_bound(low, true);

                let high = range_step.high.unwrap_or(1.);
                let high_bound = into_finite_bound(high, range_step.include_high);

                // Check this here because [`Standard`](rand::distributions::Standard) will panic
                // otherwise.
                if !low.is_finite() || !high.is_finite() {
                    return Err(anyhow!("{} range with low={} high={} is invalid", stringify!($target), low, high));
                }

                // Check this here because [`Standard`](rand::distributions::Standard) will panic
                // otherwise.
                if !(high - low).is_finite() {
                    return Err(anyhow!("{} range with low={} high={} has overflowed: try smaller bounds", stringify!($target), low, high))
                }

                let range = AnyRange::from_range_bounds(&(low_bound, high_bound));

                if range.is_empty() {
                    let step = step.map(|s| format!("step={}", s)).unwrap_or_default();
                    Err(anyhow!("{} range with {} {} is empty", stringify!($target), range, step))
                } else {
                    let low = if step.is_some() { Some(low) } else { None };
                    Ok(Self { range, low, step })
                }
            }
        }

        impl Distribution<$target> for StandardFloatRangeStep<$target> {
            fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> $target {
                let num = rng.gen_range(self.range.clone());
                if let Some(step) = self.step.as_ref() {
                    // If `step` is defined, attempt to align to it
                    let low = self.low.as_ref().unwrap();
                    (num - low).div_euclid(*step) * *step + low
                } else {
                    num
                }
            }
        }
    }
}

standard_float_range_step_impl! { f32 }
standard_float_range_step_impl! { f64 }

pub struct Incrementing<N = i64> {
    count: N,
    step: N,
    overflowed: bool,
}

impl<N> Incrementing<N>
where
    N: One,
{
    pub fn new() -> Self {
        Self {
            count: N::one(),
            step: N::one(),
            overflowed: false,
        }
    }
}

impl<N> Incrementing<N>
where
    N: One,
{
    pub fn new_at(start_at: N) -> Self {
        Self {
            count: start_at,
            step: N::one(),
            overflowed: false,
        }
    }
}

impl<N> Incrementing<N> {
    pub fn new_at_by(start_at: N, step: N) -> Self {
        Self {
            count: start_at,
            step,
            overflowed: false,
        }
    }
}

impl<N> Default for Incrementing<N>
where
    N: Zero + One,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<N> Generator for Incrementing<N>
where
    N: CheckedAdd + Copy,
{
    type Yield = N;

    type Return = Result<Never, Error>;

    fn next<R: Rng>(&mut self, _rng: &mut R) -> GeneratorState<Self::Yield, Self::Return> {
        let prev = self.count;
        match self.count.checked_add(&self.step) {
            Some(next) => {
                self.count = next;
                GeneratorState::Yielded(prev)
            }
            None => {
                if !self.overflowed {
                    self.overflowed = true;
                    GeneratorState::Yielded(self.count)
                } else {
                    GeneratorState::Complete(Err(failed_crate!(
                        target: Release,
                        "incrementing generator overflowed: try specifying the \'subtype\' parameter with a larger numerical primitive (e.g. u64)"
                    )))
                }
            }
        }
    }
}

macro_rules! number_node {
    {
        $(
            $rand:ident (
                $range:ident<$dist:ty> as $new_range:ident,
                $constant:ident as $new_constant:ident,
                $(
                    $categorical:ident as $new_categorical:ident
                )?,
                $(
                    $incrementing:ident as $new_incrementing:ident
                )?,
            ) for $ty:ty,
	    )*
    } => {
        derive_generator! {
            yield Token,
            return Result<Value, Error>,
            pub enum NumberNode {
                $($rand(Valuize<Tokenizer<$rand>, $ty>),)*
            }
        }

        $(
            derive_generator! {
                yield $ty,
                return Result<$ty, Error>,
                pub enum $rand {
                    $range(OnceInfallible<Random<$ty, $dist>>),
                    $constant(OnceInfallible<Yield<$ty>>),
                    $($categorical(OnceInfallible<Random<$ty, Categorical<$ty>>>),)?
                    $($incrementing(TryOnce<Incrementing<$ty>>),)?
                }
            }

            impl $rand {
                pub fn $new_range(range: RangeStep<$ty>) -> Result<Self, anyhow::Error> {
                    let dist = <$dist>::try_from_range(range)?;
                    Ok(Self::$range(Random::new_with(dist).infallible().try_once()))
                }

                pub fn $new_constant(value: $ty) -> Self {
                    Self::$constant(Yield::wrap(value).infallible().try_once())
                }

                $(
                    pub fn $new_categorical(cat: Categorical<$ty>) -> Self {
                        Self::$categorical(Random::new_with(cat).infallible().try_once())
                    }
                )?

                $(
                    pub fn $new_incrementing(incr: Incrementing<$ty>) -> Self {
                        Self::$incrementing(incr.try_once())
                    }
                )?
            }

            impl From<$rand> for NumberNode {
                fn from(value: $rand) -> Self {
                    Self::$rand(value.into_token().map_complete(value_from_ok_number::<$ty>))
                }
            }
        )*
    }
}

number_node!(
    RandomU64 (
        U64Range<StandardIntRangeStep<u64, u64>> as range,
        U64Constant as constant,
        U64Categorical as categorical,
        Incrementing as incrementing,
    ) for u64,
    RandomI64 (
        I64Range<StandardIntRangeStep<u64, i128>> as range,
        I64Constant as constant,
        I64Categorical as categorical,
        Incrementing as incrementing,
    ) for i64,
    RandomF64 (
        F64Range<StandardFloatRangeStep<f64>> as range,
        F64Constant as constant,,,
    ) for f64,
    RandomU32 (
        U32Range<StandardIntRangeStep<u32, u32>> as range,
        U32Constant as constant,
        U32Categorical as categorical,
        Incrementing as incrementing,
    ) for u32,
    RandomI32 (
        I32Range<StandardIntRangeStep<u32, i64>> as range,
        I32Constant as constant,
        I32Categorical as categorical,
        Incrementing as incrementing,
    ) for i32,
    RandomF32 (
        F32Range<StandardFloatRangeStep<f32>> as range,
        F32Constant as constant,,,
    ) for f32,
    RandomI16 (
        I16Range<StandardIntRangeStep<u16, i32>> as range,
        I16Constant as constant,
        I16Categorical as categorical,
        Incrementing as incrementing,
    ) for i16,
    RandomI8 (
        I8Range<StandardIntRangeStep<u8, i16>> as range,
        I8Constant as constant,
        I8Categorical as categorical,
        Incrementing as incrementing,
    ) for i8,
);

#[cfg(test)]
pub mod test {
    use super::*;
    use rand::rngs::OsRng;

    #[test]
    fn test_overflow_behaviour() {
        let mut incrementing: Incrementing<u8> = Incrementing {
            count: 0,
            step: 1,
            overflowed: false,
        };

        #[allow(clippy::default_constructed_unit_structs)]
        let mut rng = OsRng::default();

        for i in 0..255 {
            assert_eq!(i, incrementing.next(&mut rng).into_yielded().unwrap())
        }

        assert!(incrementing.next(&mut rng).into_complete().is_err())
    }
}
