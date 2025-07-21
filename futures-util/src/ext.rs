pub trait FutureExt: Future {
    fn along_with(self, other: impl Future)
    where
        Self: Sized,
    {
        // Join2
    }
    // fn then(self, other: impl Future)
    // where
    //     Self: Sized,
    // {

    // }
}
