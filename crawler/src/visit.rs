use crate::Visit;

pub trait VisitHook {
    fn visit(&mut self, url: &str, now: i64) -> (Visit, Vec<String>);
}
