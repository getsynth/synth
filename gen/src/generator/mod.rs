use std::{
    iter::{Extend, FromIterator},
    marker::PhantomData,
};

use crate::{GeneratorState, Never};

#[cfg(feature = "shared")]
use crate::Shared;

use rand::{
    distributions::{Distribution, Standard},
    Rng,
};

#[cfg(feature = "faker")]
use fake::Dummy as FakerDummy;

pub mod r#try;
pub use r#try::*;
use std::collections::VecDeque;

/// The core trait of this crate.
///
/// [`Generator`](crate::Generator)s are stateful streams that pull
/// randomness from an [`Rng`](rand::Rng) to yield a sequence of
/// values of type [`Yield`](crate::Generator::Yield). On
/// completion of the stream, they return a value of type
/// [`Return`](crate::Generator::Return).
///
/// Note that [`Generator`](crate::Generator)s are not required to
/// complete in finite time.
pub trait Generator {
    /// The type of values yielded by this generator.
    type Yield;

    /// The type of values returned by this generator on completion.
    type Return;

    /// Step through one item in the stream.
    fn next<R: Rng>(&mut self, rng: &mut R) -> GeneratorState<Self::Yield, Self::Return>;

    fn complete<R: Rng>(&mut self, rng: &mut R) -> Self::Return {
        loop {
            if let GeneratorState::Complete(ret) = self.next(rng) {
                return ret;
            }
        }
    }
}

/// A trait extension for [`Generator`](crate::Generator)s that allow
/// for composing complex streams from simpler ones.
pub trait GeneratorExt: Generator + Sized {
    /// Transform every value yielded by the stream into a returned value.
    fn once(self) -> Once<Self> {
        Once {
            inner: self,
            output: None,
        }
    }

    fn infallible<E>(self) -> Infallible<Self, E> {
        Infallible {
            inner: self,
            _error: PhantomData,
        }
    }

    /// Apply a closure to the values returned by `self`.
    fn map_complete<O, F: Fn(Self::Return) -> O>(self, closure: F) -> MapComplete<Self, F, O> {
        MapComplete {
            inner: self,
            closure,
            _output: PhantomData,
        }
    }

    /// Apply a closure to the values yielded by `self`.
    fn map_yielded<O, F: Fn(Self::Yield) -> O>(self, closure: F) -> MapYielded<Self, F, O> {
        MapYielded {
            inner: self,
            closure,
            _output: PhantomData,
        }
    }

    /// The monadic bind operation for [`Generator`](crate::Generator)s.
    ///
    /// Runs `self` to completion then runs `closure(ret)` to
    /// completion.
    fn and_then<O, F>(self, closure: F) -> AndThen<Self, F, O>
    where
        F: Fn(Self::Return) -> O,
        O: Generator<Yield = Self::Yield>,
    {
        AndThen {
            inner: self,
            closure,
            output: None,
        }
    }

    /// Concatenate `self` with `right`.
    ///
    /// First `self` is exhausted to completion, then `right` is
    /// exhausted to completion. The new
    /// [`Generator`](crate::Generator) returns a pair consisting of
    /// the values returned by `self` and `right`.
    fn concatenate<R: Generator>(self, right: R) -> Concatenate<Self, R> {
        Concatenate {
            left: self,
            left_output: None,
            right,
        }
    }

    /// Run `self` to completion, discarding intermediate yielded
    /// values and passing the returned value through.
    fn exhaust(self) -> Exhaust<Self> {
        Exhaust { inner: self }
    }

    /// Prefix the stream with another.
    #[inline]
    fn prefix<BG>(self, prefix: BG) -> Prefix<BG, Self>
    where
        BG: Generator<Yield = Self::Yield>,
    {
        self.brace(prefix, Complete::empty())
    }

    /// Suffix the stream with another.
    #[inline]
    fn suffix<EG>(self, suffix: EG) -> Suffix<Self, EG>
    where
        EG: Generator<Yield = Self::Yield>,
    {
        self.brace(Complete::empty(), suffix)
    }

    /// Brace the stream with two others.
    fn brace<BG, EG>(self, begin: BG, end: EG) -> Brace<BG, Self, EG>
    where
        BG: Generator<Yield = Self::Yield>,
        EG: Generator<Yield = Self::Yield>,
    {
        Brace {
            begin,
            inner: self,
            end,
            state: BraceState::Begin,
            complete: None,
        }
    }

    /// Run a closure on every value generated by `self` (both yielded
    /// and returned).
    ///
    /// Useful for debugging.
    fn inspect<F>(self, closure: F) -> Inspect<Self, F>
    where
        F: Fn(&GeneratorState<Self::Yield, Self::Return>),
    {
        Inspect {
            inner: self,
            closure,
        }
    }

    fn maybe(self) -> Maybe<Self> {
        Maybe {
            inner: self,
            include: false,
        }
    }

    #[cfg(feature = "shared")]
    fn shared(self) -> Shared<Self> {
        Shared::new(self)
    }

    /// Collect all values yielded by `self` into a single yielded
    /// [`Vec`](std::vec::Vec).
    fn aggregate(self) -> Aggregate<Self> {
        Aggregate {
            inner: self,
            output: None,
        }
    }

    /// Repeat `self` a total of `len` times, passing through yielded
    /// values and returning all intermediate returned values in a
    /// single [`Vec`](std::vec::Vec).
    fn repeat(self, len: usize) -> Repeat<Self> {
        Repeat {
            inner: self,
            len,
            rem: len,
            ret: Vec::new(),
        }
    }

    fn replay(self, len: usize) -> Replay<Self> {
        Replay {
            inner: self,
            len,
            idx: 0,
            rem: Some(len),
            buf: Vec::new(),
            ret: None,
        }
    }

    fn replay_forever(self) -> Replay<Self> {
        Replay {
            inner: self,
            len: 0,
            idx: 0,
            rem: None,
            buf: Vec::new(),
            ret: None,
        }
    }

    fn peekable(self) -> Peek<Self> {
        Peek {
            inner: self,
            buffer: Default::default(),
        }
    }

    fn into_iterator<'r, R: Rng>(self, rng: &'r mut R) -> Iterable<'r, Self, R> {
        Iterable {
            inner: self,
            rng,
            output: None,
        }
    }
}

impl<T> GeneratorExt for T where T: Generator {}

/// This `struct` is created by the
/// [`infallible`](crate::GeneratorExt::infallible) method on
/// [`Generator`](crate::Generator).
pub struct Infallible<G, E> {
    inner: G,
    _error: PhantomData<E>,
}

impl<G, E> Generator for Infallible<G, E>
where
    G: Generator,
{
    type Yield = G::Yield;

    type Return = Result<G::Return, E>;

    fn next<R: Rng>(&mut self, rng: &mut R) -> GeneratorState<Self::Yield, Self::Return> {
        match self.inner.next(rng) {
            GeneratorState::Yielded(y) => GeneratorState::Yielded(y),
            GeneratorState::Complete(c) => GeneratorState::Complete(Ok(c)),
        }
    }
}

/// A generator that takes values yielded by another and transforms
/// them into returned values.
///
/// This `struct` is created by the
/// [`once`](crate::GeneratorExt::once) method on
/// [`Generator`](crate::Generator).
pub struct Once<G: Generator> {
    inner: G,
    output: Option<G::Yield>,
}

impl<G> Generator for Once<G>
where
    G: Generator<Return = Never>,
    G::Yield: Clone,
{
    type Yield = G::Yield;

    type Return = G::Yield;

    fn next<R: Rng>(&mut self, rng: &mut R) -> GeneratorState<Self::Yield, Self::Return> {
        if let Some(y) = std::mem::replace(&mut self.output, None) {
            GeneratorState::Complete(y)
        } else {
            match self.inner.next(rng) {
                GeneratorState::Yielded(y) => {
                    self.output = Some(y.clone());
                    GeneratorState::Yielded(y)
                }
                GeneratorState::Complete(_) => self.next(rng),
            }
        }
    }
}

/// This `struct` is constructed by the
/// [`map_complete`](crate::GeneratorExt::map_complete) method on
/// [`Generator`](crate::Generator).
pub struct MapComplete<G, F, O> {
    inner: G,
    closure: F,
    _output: PhantomData<O>,
}

impl<G, F, O> Generator for MapComplete<G, F, O>
where
    G: Generator,
    F: Fn(G::Return) -> O,
{
    type Yield = G::Yield;

    type Return = O;

    fn next<R: Rng>(&mut self, rng: &mut R) -> GeneratorState<Self::Yield, Self::Return> {
        self.inner.next(rng).map_complete(|c| (self.closure)(c))
    }
}

impl<G, F, O> MapComplete<G, F, O> {
    pub fn into_inner(self) -> G {
        self.inner
    }
}

/// This `struct` is constructed by the
/// [`map_yielded`](crate::GeneratorExt::map_yielded) method on
/// [`Generator`](crate::Generator).
pub struct MapYielded<G, F, O> {
    inner: G,
    closure: F,
    _output: PhantomData<O>,
}

impl<G, F, O> Generator for MapYielded<G, F, O>
where
    G: Generator,
    F: Fn(G::Yield) -> O,
{
    type Yield = O;

    type Return = G::Return;

    fn next<R: Rng>(&mut self, rng: &mut R) -> GeneratorState<Self::Yield, Self::Return> {
        self.inner.next(rng).map_yielded(|y| (self.closure)(y))
    }
}

/// This `struct` is constructed by the
/// [`and_then`](crate::GeneratorExt::and_then) method on
/// [`Generator`](crate::Generator)
pub struct AndThen<G, F, O> {
    inner: G,
    closure: F,
    output: Option<O>,
}

impl<G, F, O> Generator for AndThen<G, F, O>
where
    G: Generator,
    F: Fn(G::Return) -> O,
    O: Generator<Yield = G::Yield>,
{
    type Yield = G::Yield;

    type Return = O::Return;

    fn next<R: Rng>(&mut self, rng: &mut R) -> GeneratorState<Self::Yield, Self::Return> {
        if let Some(output) = self.output.as_mut() {
            let next = output.next(rng);
            if next.is_complete() {
                self.output = None;
            }
            next
        } else {
            match self.inner.next(rng) {
                GeneratorState::Yielded(y) => GeneratorState::Yielded(y),
                GeneratorState::Complete(r) => {
                    self.output = Some((self.closure)(r));
                    self.next(rng)
                }
            }
        }
    }
}

/// This `struct` is constructed by the
/// [`concatenate`](crate::GeneratorExt::concatenate) method on
/// [`Generator`](crate::Generator)
pub struct Concatenate<Left: Generator, Right: Generator> {
    left: Left,
    left_output: Option<Left::Return>,
    right: Right,
}

impl<Left, Right> Generator for Concatenate<Left, Right>
where
    Left: Generator,
    Right: Generator<Yield = Left::Yield>,
{
    type Yield = Left::Yield;

    type Return = (Left::Return, Right::Return);

    fn next<R: Rng>(&mut self, rng: &mut R) -> GeneratorState<Self::Yield, Self::Return> {
        if self.left_output.is_none() {
            match self.left.next(rng) {
                GeneratorState::Complete(r) => {
                    self.left_output = Some(r);
                    self.next(rng)
                }
                GeneratorState::Yielded(y) => GeneratorState::Yielded(y),
            }
        } else {
            match self.right.next(rng) {
                GeneratorState::Complete(right) => {
                    let left = std::mem::replace(&mut self.left_output, None).unwrap();
                    GeneratorState::Complete((left, right))
                }
                GeneratorState::Yielded(y) => GeneratorState::Yielded(y),
            }
        }
    }

    fn complete<R: Rng>(&mut self, rng: &mut R) -> Self::Return {
        loop {
            if let GeneratorState::Complete(ret) = self.next(rng) {
                return ret;
            }
        }
    }
}

/// This `struct` is constructed by the
/// [`exhaust`](crate::GeneratorExt::exhaust) method on
/// [`Generator`](crate::Generator)
pub struct Exhaust<G> {
    inner: G,
}

impl<G> Generator for Exhaust<G>
where
    G: Generator,
{
    type Yield = G::Yield;

    type Return = G::Return;

    fn next<R: Rng>(&mut self, rng: &mut R) -> GeneratorState<Self::Yield, Self::Return> {
        GeneratorState::Complete(self.inner.complete(rng))
    }
}

/// This `struct` is created by the
/// [`suffix`](crate::GeneratorExt::suffix) method on
/// [`Generator`](crate::Generator).
pub type Suffix<G: Generator, EG: Generator<Yield = G::Yield>> = Brace<Complete<G::Yield>, G, EG>;

/// This `struct` is created by the
/// [`prefix`](crate::GeneratorExt::suffix) method on
/// [`Generator`](crate::Generator).
pub type Prefix<BG: Generator<Yield = G::Yield>, G: Generator> = Brace<BG, G, Complete<G::Yield>>;

enum BraceState {
    Begin,
    Middle,
    End,
}

/// This `struct` is created by the
/// [`brace`](crate::GeneratorExt::suffix) method on
/// [`Generator`](crate::Generator).
pub struct Brace<BG, G, EG>
where
    BG: Generator,
    G: Generator,
    EG: Generator,
{
    begin: BG,
    pub inner: G,
    end: EG,
    state: BraceState,
    complete: Option<G::Return>,
}

impl<BG, G, EG> Generator for Brace<BG, G, EG>
where
    BG: Generator<Yield = G::Yield>,
    G: Generator,
    EG: Generator<Yield = G::Yield>,
{
    type Yield = G::Yield;

    type Return = G::Return;

    fn next<R: Rng>(&mut self, rng: &mut R) -> GeneratorState<Self::Yield, Self::Return> {
        match self.state {
            BraceState::Begin => match self.begin.next(rng) {
                GeneratorState::Complete(_) => {
                    self.state = BraceState::Middle;
                    self.next(rng)
                }
                GeneratorState::Yielded(y) => GeneratorState::Yielded(y),
            },
            BraceState::Middle => match self.inner.next(rng) {
                GeneratorState::Complete(r) => {
                    self.complete = Some(r);
                    self.state = BraceState::End;
                    self.next(rng)
                }
                GeneratorState::Yielded(y) => GeneratorState::Yielded(y),
            },
            BraceState::End => match self.end.next(rng) {
                GeneratorState::Complete(_) => {
                    self.state = BraceState::Begin;
                    let r = std::mem::replace(&mut self.complete, None).unwrap();
                    GeneratorState::Complete(r)
                }
                GeneratorState::Yielded(y) => GeneratorState::Yielded(y),
            },
        }
    }
}

/// This `struct` is created by the
/// [`inspect`](crate::GeneratorExt::inspect) method on
/// [`Generator`](crate::Generator).
pub struct Inspect<G, F> {
    inner: G,
    closure: F,
}

impl<G, F> Generator for Inspect<G, F>
where
    G: Generator,
    F: Fn(&GeneratorState<G::Yield, G::Return>),
{
    type Yield = G::Yield;

    type Return = G::Return;

    fn next<R: Rng>(&mut self, rng: &mut R) -> GeneratorState<Self::Yield, Self::Return> {
        let passthrough = self.inner.next(rng);
        (self.closure)(&passthrough);
        passthrough
    }
}

/// This `struct` is created by the
/// [`aggregate`](crate::GeneratorExt::aggregate) method on
/// [`Generator`](crate::Generator).
pub struct Aggregate<G: Generator> {
    inner: G,
    output: Option<G::Return>,
}

impl<G> Generator for Aggregate<G>
where
    G: Generator,
{
    type Yield = Vec<G::Yield>;

    type Return = G::Return;

    fn next<R: Rng>(&mut self, rng: &mut R) -> GeneratorState<Self::Yield, Self::Return> {
        if let Some(r) = std::mem::replace(&mut self.output, None) {
            GeneratorState::Complete(r)
        } else {
            let mut out = Vec::new();
            loop {
                match self.inner.next(rng) {
                    GeneratorState::Yielded(y) => out.push(y),
                    GeneratorState::Complete(r) => {
                        self.output = Some(r);
                        break;
                    }
                }
            }
            GeneratorState::Yielded(out)
        }
    }
}

/// This `struct` is created by the
/// [`repeat`](crate::GeneratorExt::repeat) method on
/// [`Generator`](crate::Generator).
pub struct Repeat<G>
where
    G: Generator,
{
    inner: G,
    len: usize,
    rem: usize,
    ret: Vec<G::Return>,
}

impl<G> Generator for Repeat<G>
where
    G: Generator,
{
    type Yield = G::Yield;

    type Return = Vec<G::Return>;

    fn next<R: Rng>(&mut self, rng: &mut R) -> GeneratorState<Self::Yield, Self::Return> {
        if self.rem != 0 {
            match self.inner.next(rng) {
                GeneratorState::Yielded(y) => GeneratorState::Yielded(y),
                GeneratorState::Complete(r) => {
                    self.rem -= 1;
                    self.ret.push(r);
                    self.next(rng)
                }
            }
        } else {
            self.rem = self.len;
            let ret = std::mem::replace(&mut self.ret, Vec::new());
            GeneratorState::Complete(ret)
        }
    }
}

/// This `struct` is created by the
/// [`replay`](crate::GeneratorExt::replay) method on
/// [`Generator`](crate::Generator).
pub struct Replay<G: Generator> {
    inner: G,
    len: usize,
    rem: Option<usize>,
    idx: usize,
    buf: Vec<G::Yield>,
    ret: Option<G::Return>,
}

impl<G: Generator> Replay<G> {
    fn purge(&mut self) {
        self.buf = Vec::new();
        self.ret = None;
        self.idx = 0;

        let len = self.len;
        self.rem.as_mut().map(|rem| *rem = len);
    }
}

impl<G> Generator for Replay<G>
where
    G: Generator,
    G::Yield: Clone,
    G::Return: Clone,
{
    type Yield = G::Yield;

    type Return = G::Return;

    fn next<R: Rng>(&mut self, rng: &mut R) -> GeneratorState<Self::Yield, Self::Return> {
        if self.ret.is_some() {
            let mut rem = self.rem;
            if rem.as_ref().map(|rem| *rem > 0).unwrap_or(true) {
                if let Some(next) = self.buf.get(self.idx) {
                    self.idx += 1;
                    GeneratorState::Yielded(next.clone())
                } else {
                    self.idx = 0;
                    rem.as_mut().map(|inner| *inner -= 1);
                    GeneratorState::Complete(self.ret.clone().unwrap())
                }
            } else {
                self.purge();
                self.next(rng)
            }
        } else {
            match self.inner.next(rng) {
                GeneratorState::Yielded(yielded) => {
                    self.buf.push(yielded.clone());
                    GeneratorState::Yielded(yielded)
                }
                GeneratorState::Complete(complete) => {
                    self.ret = Some(complete.clone());
                    GeneratorState::Complete(complete)
                }
            }
        }
    }
}

/// A [`Generator`](crate::Generator) that chains the generators in a
/// collection, returning a [`Vec`](std::vec::Vec) of the returned
/// values.
///
/// [`Chain`](Chain) can be built using
/// [`FromIterator`](std::iter::FromIterator).
pub struct Chain<G>
where
    G: Generator,
{
    pub inners: Vec<G>,
    idx: usize,
    completed: Vec<G::Return>,
}

impl<G> Generator for Chain<G>
where
    G: Generator,
{
    type Yield = G::Yield;

    type Return = Vec<G::Return>;

    fn next<R: Rng>(&mut self, rng: &mut R) -> GeneratorState<Self::Yield, Self::Return> {
        if self.idx == self.inners.len() {
            let out = std::mem::replace(&mut self.completed, Vec::new());
            self.idx = 0;
            GeneratorState::Complete(out)
        } else {
            let gen = self.inners.get_mut(self.idx).unwrap();
            match gen.next(rng) {
                GeneratorState::Yielded(y) => GeneratorState::Yielded(y),
                GeneratorState::Complete(r) => {
                    self.idx += 1;
                    self.completed.push(r);
                    self.next(rng)
                }
            }
        }
    }
}

impl<G> FromIterator<G> for Chain<G>
where
    G: Generator,
{
    fn from_iter<T: IntoIterator<Item = G>>(iter: T) -> Self {
        Self {
            inners: iter.into_iter().collect(),
            idx: 0,
            completed: Vec::new(),
        }
    }
}

impl<G> Extend<G> for Chain<G>
where
    G: Generator,
{
    fn extend<T>(&mut self, iter: T)
    where
        T: IntoIterator<Item = G>,
    {
        self.inners.extend(iter)
    }
}

/// A [`Generator`](crate::Generator) that randomly exhausts one of
/// the generators in a collection.
///
/// [`OneOf`](OneOf) can be built using
/// [`FromIterator`](std::iter::FromIterator).
pub struct OneOf<G> {
    inners: Vec<G>,
    cursor: Option<(usize, Box<G>)>,
}

impl<G> Generator for OneOf<G>
where
    G: Generator,
{
    type Yield = G::Yield;

    type Return = Option<G::Return>;

    fn next<R: Rng>(&mut self, rng: &mut R) -> GeneratorState<Self::Yield, Self::Return> {
        if let Some((_, picked)) = self.cursor.as_mut() {
            let next = picked.next(rng);
            if next.is_complete() {
                let (idx, picked) = std::mem::replace(&mut self.cursor, None).unwrap();
                self.inners.insert(idx, *picked);
            }
            next.map_complete(|c| Some(c))
        } else {
            if self.inners.is_empty() {
                GeneratorState::Complete(None)
            } else {
                let idx = rng.gen_range(0..self.inners.len());
                self.cursor = Some((idx, Box::new(self.inners.remove(idx))));
                self.next(rng)
            }
        }
    }
}

impl<G> FromIterator<G> for OneOf<G>
where
    G: Generator,
{
    fn from_iter<T: IntoIterator<Item = G>>(iter: T) -> Self {
        Self {
            inners: iter.into_iter().collect(),
            cursor: None,
        }
    }
}

/// A [`Generator`](crate::Generator) that optionally exhausts
/// another.
pub struct Maybe<G>
where
    G: Generator,
{
    inner: G,
    include: bool,
}

impl<G> Generator for Maybe<G>
where
    G: Generator,
{
    type Yield = G::Yield;

    type Return = Option<G::Return>;

    fn next<R: Rng>(&mut self, rng: &mut R) -> GeneratorState<Self::Yield, Self::Return> {
        if self.include {
            let next = self.inner.next(rng);
            if next.is_complete() {
                self.include = false;
            }
            next.map_complete(|c| Some(c))
        } else {
            self.include = rng.gen();
            if self.include {
                self.next(rng)
            } else {
                GeneratorState::Complete(None)
            }
        }
    }
}

/// A generator of dummy values, generated by the [`fake`](fake) crate.
#[cfg(feature = "faker")]
pub struct Dummy<T, D>(D, PhantomData<T>);

#[cfg(feature = "faker")]
impl<D> Dummy<(), D> {
    /// Create a seed of dummy values of type `T` with [`fake::Dummy`](fake::Dummy) `D`.
    ///
    /// See [fake::faker](fake::faker) for a list of available dummies.
    ///
    /// # Example
    /// ```
    /// # use synth_gen::prelude::*;
    /// # use rand::thread_rng;
    /// # fn main() {
    /// let first_name: String = synth_gen::generator::Dummy::new(faker::name::en::FirstName())
    ///     .once()
    ///     .complete(&mut thread_rng());
    /// println!("{}", first_name)
    /// # }
    pub fn new<TT>(dummy: D) -> Dummy<TT, D>
    where
        TT: FakerDummy<D>,
    {
        Dummy(dummy, PhantomData)
    }
}

#[cfg(feature = "faker")]
impl<T, D> Generator for Dummy<T, D>
where
    T: FakerDummy<D>,
{
    type Yield = T;

    type Return = Never;

    fn next<R: Rng>(&mut self, rng: &mut R) -> GeneratorState<Self::Yield, Self::Return> {
        GeneratorState::Yielded(T::dummy_with_rng(&self.0, rng))
    }
}

/// Creates a [`Dummy`](Dummy) generator from a
/// [`fake::Dummy`](fake::Dummy).
#[cfg(feature = "faker")]
pub fn dummy<T, D>(dummy: D) -> Dummy<T, D>
where
    T: FakerDummy<D>,
{
    Dummy::new(dummy)
}

/// A primitive random value generator that yields from a
/// [`rand::Distribution`](rand::Distribution).
pub struct Random<T, D = Standard>(D, PhantomData<T>);

impl<D> Random<(), D> {
    /// Create a new seed from a distribution `D`.
    pub fn new_with<TT>(dist: D) -> Random<TT, D>
    where
        D: Distribution<TT>,
    {
        Random(dist, PhantomData)
    }
}

impl Random<()> {
    pub fn new<T>() -> Random<T>
    where
        Standard: Distribution<T>,
    {
        Random::new_with(Standard)
    }
}

impl<D, T> Generator for Random<T, D>
where
    D: Distribution<T>,
{
    type Yield = T;

    type Return = Never;

    fn next<R: Rng>(&mut self, rng: &mut R) -> GeneratorState<Self::Yield, Self::Return> {
        GeneratorState::Yielded(self.0.sample(rng))
    }
}

/// Create a seed of random values of `T` with `rand::Distribution`
/// `D`.
pub fn random<T, D: Distribution<T>>(dist: D) -> Random<T, D> {
    Random::new_with(dist)
}

impl<BG, MG, EG> Extend<MG> for Brace<BG, Chain<MG>, EG>
where
    BG: Generator<Yield = MG::Yield>,
    MG: Generator,
    EG: Generator<Yield = MG::Yield>,
{
    fn extend<T: IntoIterator<Item = MG>>(&mut self, iter: T) {
        self.inner.extend(iter)
    }
}

/// A [`Generator`](crate::Generator) that yields clones of a given
/// value.
pub struct Yield<Y, C = Never> {
    _return: PhantomData<C>,
    output: Y,
}

impl<Y> Yield<Y, Never> {
    pub fn wrap(y: Y) -> Self {
        Self {
            _return: PhantomData,
            output: y,
        }
    }
}

impl<Y, C> Generator for Yield<Y, C>
where
    Y: Clone,
{
    type Yield = Y;

    type Return = C;

    fn next<R: Rng>(&mut self, _rng: &mut R) -> GeneratorState<Self::Yield, Self::Return> {
        GeneratorState::Yielded(self.output.clone())
    }
}

/// A [`Generator`](crate::Generator) that completes in one step,
/// returning a clone of a given value.
pub struct Complete<Y, C = ()> {
    _yielded: PhantomData<Y>,
    output: C,
}

impl<Y> Complete<Y> {
    pub fn empty() -> Self {
        Self {
            _yielded: PhantomData,
            output: (),
        }
    }
}

impl<Y, C> Complete<Y, C> {
    pub fn wrap(c: C) -> Self {
        Self {
            _yielded: PhantomData,
            output: c,
        }
    }
}

impl<Y, C> Generator for Complete<Y, C>
where
    C: Clone,
{
    type Yield = Y;

    type Return = C;

    fn next<R: Rng>(&mut self, _rng: &mut R) -> GeneratorState<Self::Yield, Self::Return> {
        GeneratorState::Complete(self.output.clone())
    }
}

/// A wrapper that allows peeking at the next (upcoming) value of a
/// generator without consuming it.
///
/// This `struct` is created by the
/// [`peekable`](crate::GeneratorExt::peekable) method on
/// [`Generator`](crate::Generator).
pub struct Peek<G: Generator> {
    inner: G,
    buffer: VecDeque<GeneratorState<G::Yield, G::Return>>,
}

impl<G> Generator for Peek<G>
where
    G: Generator,
{
    type Yield = G::Yield;

    type Return = G::Return;

    fn next<R: Rng>(&mut self, rng: &mut R) -> GeneratorState<Self::Yield, Self::Return> {
        if let Some(next) = self.buffer.pop_front() {
            next
        } else {
            self.inner.next(rng)
        }
    }
}

impl<G> PeekableGenerator for Peek<G>
where
    G: Generator,
{
    fn peek<R: Rng>(&mut self, rng: &mut R) -> &GeneratorState<G::Yield, G::Return> {
        let next = self.inner.next(rng);
        self.buffer.push_back(next);
        self.buffer.back().unwrap()
    }

    fn peek_next<R: Rng>(&mut self, rng: &mut R) -> &GeneratorState<G::Yield, G::Return> {
        if self.buffer.is_empty() {
            let next = self.inner.next(rng);
            self.buffer.push_back(next);
        }
        self.buffer.front().unwrap()
    }
}

/// A [`Generator`](crate::Generator) that allows for peeking at the
/// upcoming values without consuming them.
pub trait PeekableGenerator: Generator {
    fn peek<R: Rng>(&mut self, rng: &mut R) -> &GeneratorState<Self::Yield, Self::Return>;
    fn peek_next<R: Rng>(&mut self, rng: &mut R) -> &GeneratorState<Self::Yield, Self::Return>;
}

/// A convenience generator that is equivalent to
/// `Yield::wrap(...).once()`
pub type Just<C> = Once<Yield<C, Never>>;

pub struct Iterable<'r, G, R>
where
    G: Generator,
    R: Rng,
{
    inner: G,
    rng: &'r mut R,
    output: Option<G::Return>,
}

impl<'r, G, R> Iterable<'r, G, R>
where
    G: Generator,
    R: Rng,
{
    pub fn restart(&mut self) -> G::Return {
        if let Some(r) = std::mem::replace(&mut self.output, None) {
            r
        } else {
            while self.next().is_some() {}
            self.restart()
        }
    }
}

impl<'r, G, R> std::iter::Iterator for Iterable<'r, G, R>
where
    G: Generator,
    R: Rng,
{
    type Item = G::Yield;

    fn next(&mut self) -> Option<Self::Item> {
        if self.output.is_none() {
            match self.inner.next(self.rng) {
                GeneratorState::Yielded(y) => Some(y),
                GeneratorState::Complete(c) => {
                    self.output = Some(c);
                    None
                }
            }
        } else {
            None
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    use crate::Never;

    use rand::rngs::ThreadRng;

    #[inline]
    pub fn prime<T>(t: T) -> (Yield<T, Never>, ThreadRng)
    where
        T: Clone,
    {
        (Yield::wrap(t), rand::thread_rng())
    }

    #[test]
    fn once() {
        let (seed, mut rng) = prime(42);
        let mut subject = seed.once();
        assert!(subject.next(&mut rng).is_yielded());
        assert!(subject.next(&mut rng).is_complete());
    }

    #[test]
    fn map() {
        let (seed, mut rng) = prime(42);
        let mut subject = seed.once().map_complete(|value| value - 42);
        assert_eq!(subject.next(&mut rng), GeneratorState::Yielded(42));
        assert_eq!(subject.next(&mut rng), GeneratorState::Complete(0));
    }

    #[test]
    fn and_then() {
        let (seed, mut rng) = prime(42);
        let mut subject = seed.once().and_then(|value| Yield::wrap(value - 42).once());
        assert_eq!(subject.next(&mut rng), GeneratorState::Yielded(42));
        assert_eq!(subject.next(&mut rng), GeneratorState::Yielded(0));
    }

    #[test]
    fn intercept() {
        let (seed, mut rng) = prime(42);
        let mut subject = seed.once().map_yielded(|value| value - 42);
        assert_eq!(subject.next(&mut rng), GeneratorState::Yielded(0));
        assert_eq!(subject.next(&mut rng), GeneratorState::Complete(42));
    }

    #[test]
    fn concatenate() {
        let (seed, mut rng) = prime(42);
        let mut subject = seed.once().concatenate(Yield::wrap(84).once());
        assert_eq!(subject.next(&mut rng), GeneratorState::Yielded(42));
        assert_eq!(subject.next(&mut rng), GeneratorState::Yielded(84));
        assert_eq!(subject.next(&mut rng), GeneratorState::Complete((42, 84)));
    }

    #[test]
    fn exhaust() {
        let (seed, mut rng) = prime(42);
        let mut subject = seed.once().exhaust();
        assert_eq!(subject.next(&mut rng), GeneratorState::Complete(42));
    }

    #[test]
    fn brace() {
        let (seed, mut rng) = prime(42);
        let mut subject = seed
            .once()
            .brace(Yield::wrap(-42).once(), Yield::wrap(84).once());
        assert_eq!(subject.next(&mut rng), GeneratorState::Yielded(-42));
        assert_eq!(subject.next(&mut rng), GeneratorState::Yielded(42));
        assert_eq!(subject.next(&mut rng), GeneratorState::Yielded(84));
        assert_eq!(subject.next(&mut rng), GeneratorState::Complete(42));
    }

    #[test]
    fn aggregate() {
        let (seed, mut rng) = prime(42);
        let mut subject = seed.once().repeat(5).aggregate();
        assert_eq!(subject.complete(&mut rng), vec![42, 42, 42, 42, 42]);
    }

    #[test]
    fn take() {
        let (seed, mut rng) = prime(42);
        let mut subject = seed.once().repeat(2);
        assert_eq!(subject.next(&mut rng), GeneratorState::Yielded(42));
        assert_eq!(subject.next(&mut rng), GeneratorState::Yielded(42));
        assert_eq!(
            subject.next(&mut rng),
            GeneratorState::Complete(vec![42, 42])
        );
    }

    #[test]
    fn one_of() {
        let (seed, mut rng) = prime(42i32);
        let mut subject = vec![seed.once()].into_iter().collect::<OneOf<_>>();
        assert_eq!(subject.next(&mut rng), GeneratorState::Yielded(42));
        assert_eq!(subject.next(&mut rng), GeneratorState::Complete(Some(42)));
    }

    #[test]
    fn replay() {
        let mut rng = rand::thread_rng();
        let mut gen = Random::new::<i32>().once().replay(5);
        let mut buf = Vec::new();

        let mut is_complete = false;
        while !is_complete {
            let next = gen.next(&mut rng);
            is_complete = next.is_complete();
            buf.push(next);
        }

        let mut iter_buf = buf.iter();
        let mut restarts = 1;
        while restarts <= 10 {
            let next_buf = match iter_buf.next() {
                Some(next) => next,
                None => {
                    restarts += 1;
                    iter_buf = buf.iter();
                    iter_buf.next().unwrap()
                }
            };
            assert_eq!(*next_buf, gen.next(&mut rng))
        }

        for item in buf {
            assert!(item != gen.next(&mut rng))
        }
    }

    #[test]
    fn peek() {
        impl<T> Generator for Vec<T> {
            type Yield = T;
            type Return = Never;

            fn next<R: Rng>(&mut self, _rng: &mut R) -> GeneratorState<Self::Yield, Self::Return> {
                match self.pop() {
                    None => unreachable!(),
                    Some(v) => GeneratorState::Yielded(v),
                }
            }
        }
        let gen = vec![1, 2, 3];
        let mut rng = rand::thread_rng();
        let mut peekable = gen.peekable();
        assert_eq!(&GeneratorState::Yielded(3), peekable.peek(&mut rng));
        assert_eq!(&GeneratorState::Yielded(2), peekable.peek(&mut rng));
        assert_eq!(&GeneratorState::Yielded(1), peekable.peek(&mut rng));
        assert_eq!(&GeneratorState::Yielded(3), peekable.peek_next(&mut rng));
        assert_eq!(GeneratorState::Yielded(3), peekable.next(&mut rng));
        assert_eq!(&GeneratorState::Yielded(2), peekable.peek_next(&mut rng));
        assert_eq!(&GeneratorState::Yielded(2), peekable.peek_next(&mut rng));
        assert_eq!(GeneratorState::Yielded(2), peekable.next(&mut rng));
        assert_eq!(&GeneratorState::Yielded(1), peekable.peek_next(&mut rng));
        assert_eq!(GeneratorState::Yielded(1), peekable.next(&mut rng));
    }
}
