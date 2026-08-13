use worker::Env;

pub(super) struct ClientMailDispatchAdapter<'a> {
    _env: &'a Env,
}
