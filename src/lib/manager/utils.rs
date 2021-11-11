use frunk_core::hlist::HMappable;
use frunk_core::traits::{Func, Poly};

trait ToOptionHList {
    type OptionHList;
    fn to_option_hlist(self) -> Self::OptionHList;
}

struct OptionPure;
impl<T> Func<T> for OptionPure {
    type Output = Option<T>;

    fn call(i: T) -> Self::Output {
        Some(i)
    }
}

impl<T> ToOptionHList for T
where
    T: HMappable<Poly<OptionPure>>,
{
    type OptionHList = T::Output;

    fn to_option_hlist(self) -> Self::OptionHList {
        self.map(Poly(OptionPure))
    }
}
