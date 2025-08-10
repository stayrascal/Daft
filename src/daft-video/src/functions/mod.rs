use daft_dsl::functions::FunctionModule;

pub struct VideoFunctions;

impl FunctionModule for VideoFunctions {
    fn register(parent: &mut daft_dsl::functions::FunctionRegistry) {}
}
