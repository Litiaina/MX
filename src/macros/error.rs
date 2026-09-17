#[macro_export]
macro_rules! report_error {
    ($reason:expr, $context_type:expr, $context_name:expr) => {{
        tracing::error!(
            "error: '{}' | {}: '{}' | module: '{}' | file: '{}' | line: {}",
            $reason,
            $context_type,
            $context_name,
            module_path!(),
            file!(),
            line!(),
        );
    }};
}

#[macro_export]
macro_rules! fatal_error {
    ($reason:expr, $context_type:expr, $context_name:expr) => {{
        panic!(
            "error: '{}' | {}: '{}' | module: '{}' | file: '{}' | line: {}",
            $reason,
            $context_type,
            $context_name,
            module_path!(),
            file!(),
            line!(),
        );
    }};
}
