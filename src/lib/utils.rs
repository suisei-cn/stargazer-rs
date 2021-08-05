pub trait TypeEq {
    type Other;
}

impl<T> TypeEq for T {
    type Other = T;
}
