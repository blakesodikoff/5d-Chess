// use super::*;
use std::time::{Instant, Duration};

pub trait TimedFilterTrait : Iterator {
    /**
        Returns an filtered iterator, which only yields the elements of the current iterator if:
        - `condition(item) == true`
        - The time since the first call to `next()` does not exceeds `duration`

        See `TimedFilter` for more information.
    **/
    fn filter_timed<F>(self, condition: F, duration: Duration) -> TimedFilter<Self, F>
    where
        for<'b> F: Fn(&'b Self::Item) -> bool,
        Self: Sized
    {
        TimedFilter::new(self, condition, duration)
    }

    /**
        Returns an filtered iterator, which only yields the elements of the current iterator if:
        - `condition(item) == true`
        - The sum of the time taken by `condition` does not exceeds `duration`

        See `SigmaFilter` for more information.
    **/
    fn filter_sigma<F>(self, condition: F, duration: Duration) -> SigmaFilter<Self, F>
    where
        for<'b> F: Fn(&'b Self::Item) -> bool,
        Self: Sized
    {
        SigmaFilter::new(self, condition, duration)
    }
}

impl<T: ?Sized> TimedFilterTrait for T where T: Iterator {}

/**
    A variant of `Iterator::filter`, which has a maximum duration.
    It measures the elapsed time from the first call to `next` and stops when the maximum duration is reached.

    You should create instances of it by calling the `filter_timed` function on any iterator, given that `TimedFilterTrait` is
    loaded in your context (it is by default included in `prelude::*`).

    Note that this iterator does *not* measure the summation of the time taken by the filter function, but instead naively measures
    the elapsed time since the first call to `next`.
    If you wish to have the former, use `SigmaFilter` instead.
**/
pub struct TimedFilter<J, F>
where
    J: Iterator,
    for<'b> F: Fn(&'b J::Item) -> bool,
{
    pub iterator: J,
    pub condition: F,
    pub start: Option<Instant>,
    pub duration: Duration,
}

impl<J, F> TimedFilter<J, F>
where
    J: Iterator,
    F: for<'b> Fn(&'b J::Item) -> bool,
{
    pub fn new(iterator: J, condition: F, duration: Duration) -> Self {
        Self {
            iterator,
            condition,
            start: None,
            duration
        }
    }

    pub fn with_start(iterator: J, condition: F, start: Option<Instant>, duration: Duration) -> Self {
        Self {
            iterator,
            condition,
            start,
            duration
        }
    }

    pub fn elapsed(&self) -> Option<Duration> {
        match self.start {
            Some(instant) => Some(instant.elapsed()),
            None => None,
        }
    }

    pub fn remaining(&self) -> Option<Duration> {
        match self.start {
            Some(instant) => {
                let elapsed = instant.elapsed();
                if elapsed > self.duration {
                    Some(Duration::new(0, 0))
                } else {
                    Some(self.duration - elapsed)
                }
            }
            None => None,
        }
    }
}

impl<J, F> Iterator for TimedFilter<J, F>
where
    J: Iterator,
    F: for<'b> Fn(&'b J::Item) -> bool,
{
    type Item = J::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if self.start.is_none() {
            self.start = Some(Instant::now());
        }

        loop {
            if self.start.unwrap().elapsed() > self.duration {
                return None
            }
            match self.iterator.next() {
                Some(item) => {
                    if (self.condition)(&item) {
                        return Some(item)
                    }
                }
                None => return None
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self.remaining() {
            Some(d) => {
                if d == Duration::new(0, 0) {
                    return (0, Some(0))
                }
            }
            _ => {}
        }

        (0, self.iterator.size_hint().1)
    }
}

/**
    A variant of `Iterator::filter`, which limits the time that the filter function may take.
    It measures the sum of the elapsed time taken by the filter function and stops once it exceeds the given, maximum duration.

    You should create instances of it by calling the `filter_sigma` function on any iterator, given that `TimedFilterTrait` is
    loaded in your context (it is by default included in `prelude::*`).

    Note that this iterator does *not* measure the elapsed time since the first call to `next`, but instead the summation of the time taken by the filter function.

    If you wish to have the former, use `TimedFilter` instead.
**/
pub struct SigmaFilter<J, F>
where
    J: Iterator,
    for<'b> F: Fn(&'b J::Item) -> bool,
{
    pub iterator: J,
    pub condition: F,
    pub sigma: Duration,
    pub duration: Duration,
}

impl<J, F> SigmaFilter<J, F>
where
    J: Iterator,
    F: for<'b> Fn(&'b J::Item) -> bool,
{
    pub fn new(iterator: J, condition: F, duration: Duration) -> Self {
        Self {
            iterator,
            condition,
            sigma: Duration::new(0, 0),
            duration
        }
    }

    pub fn with_sigma(iterator: J, condition: F, sigma: Duration, duration: Duration) -> Self {
        Self {
            iterator,
            condition,
            sigma,
            duration
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.sigma
    }

    pub fn remaining(&self) -> Duration {
        if self.sigma > self.duration {
            Duration::new(0, 0)
        } else {
            self.duration - self.sigma
        }
    }
}

impl<J, F> Iterator for SigmaFilter<J, F>
where
    J: Iterator,
    F: for<'b> Fn(&'b J::Item) -> bool,
{
    type Item = J::Item;

    fn next(&mut self) -> Option<Self::Item> {
        let start = Instant::now();

        let res = loop {
            if self.sigma + start.elapsed() > self.duration {
                break None
            }
            match self.iterator.next() {
                Some(item) => {
                    if (self.condition)(&item) {
                        break Some(item)
                    }
                }
                None => break None
            }
        };

        self.sigma += start.elapsed();
        res
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.sigma > self.duration {
            (0, Some(0))
        } else {
            (0, self.iterator.size_hint().1)
        }
    }
}