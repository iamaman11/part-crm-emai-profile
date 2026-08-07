use profile_platform_primitives::UnixMillis;

pub trait ClockPort {
    fn now(&self) -> UnixMillis;
}
