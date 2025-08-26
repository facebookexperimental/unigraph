/// Similar to `anyhow::bail!`. Takes two arguments
/// Display - a very short and ideally consistent error type/category
/// that we can use to group errors together
#[macro_export]
macro_rules! bail {
    ( $display:expr $(,)? ) => {{
        let display: String = $display.into();
        return Err(anyhow::anyhow!($crate::UnigraphError {
            display,
            debug: None,
        }));
    }};

    ( $display:expr, $debug:literal, $($arg:tt)* ) => {{
        let display: String = $display.into();
        return Err(anyhow::anyhow!($crate::UnigraphError {
            display,
            debug: Some(format!($debug, $($arg)*)),
        }));
    }};

    ( $display:expr, $debug:literal $(,)? ) => {{
        let display: String = $display.into();
        let debug: String = $debug.into();
        return Err(anyhow::anyhow!($crate::UnigraphError {
            display,
            debug: Some(debug),
        }));
    }};
}
