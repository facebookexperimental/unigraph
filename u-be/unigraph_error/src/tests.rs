use anyhow::Context;
use anyhow::Result;
use k9::*;

use crate::*;

fn bails() -> Result<()> {
    bail!("Unigraph broke");
}

fn adds_context() -> Result<()> {
    bails().context("Added context")?;
    Ok(())
}

fn more_layers_of_context() -> Result<()> {
    adds_context()
        .context("More context")
        .with_context(|| format!("Some extensive multiline context:\n\n{:?}\n", vec![1, 2, 3]))
        .context("Another layer of context")
        .context("Lots of trailing newlines that should be trimmed\n\n\n\n")
        .with_context(|| {
            format!(
                "One of those weird errors
with weird indentation and stuff
and some random multiline debug info:\n{:#?}",
                vec![Some(1), None, Some(9999)]
            )
        })
        .context("And even more stuff")
}

fn anyhow_err() -> Result<()> {
    anyhow::bail!("Root cause that used anyhow::bail!()");
}

fn more_anyhow_context() -> Result<()> {
    anyhow_err()
        .context("more context")
        .context("some stuff")
        .context("some high level thing failed")?;
    Ok(())
}

#[test]
fn basic_test() -> Result<()> {
    let err = more_layers_of_context().unwrap_err();

    snapshot!(
        format_for_user(&err),
        "
Unigraph broke
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
<UnigraphError did not provide any debug info>

~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
Error chain and added context:

[0]: Added context
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
[1]: More context
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
[2]: Some extensive multiline context:

[1, 2, 3]
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
[3]: Another layer of context
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
[4]: Lots of trailing newlines that should be trimmed
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
[5]: One of those weird errors
with weird indentation and stuff
and some random multiline debug info:
[
    Some(
        1,
    ),
    None,
    Some(
        9999,
    ),
]
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
[6]: And even more stuff
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

"
    );

    let err = more_anyhow_context().unwrap_err();
    snapshot!(
        format_for_user(&err),
        "
Root cause that used anyhow::bail!()
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
Error chain and added context:

[0]: more context
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
[1]: some stuff
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
[2]: some high level thing failed
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

"
    );
    Ok(())
}

#[test]
fn colored_test() -> Result<()> {
    // Some environments disable colors and it makes the test break.
    // This setting will force colors to always be present.
    // I'm a bit worried about introducing other flaky tests, since this option
    // is global and will likely leak to other tests, but it's a pretty rare
    // thing to rely on.
    colored::control::set_override(true);
    let err = more_anyhow_context().unwrap_err();

    snapshot!(
        format!("{}", format_with_colors(&err)),
        r"
\u{1b}[1;31mRoot cause that used anyhow::bail!()\u{1b}[0m
\u{1b}[2m~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~\u{1b}[0m
\u{1b}[2mError chain and added context:
\u{1b}[0m
\u{1b}[2m[0]:\u{1b}[0m \u{1b}[31mmore context\u{1b}[0m
\u{1b}[2m~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~\u{1b}[0m
\u{1b}[2m[1]:\u{1b}[0m \u{1b}[31msome stuff\u{1b}[0m
\u{1b}[2m~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~\u{1b}[0m
\u{1b}[2m[2]:\u{1b}[0m \u{1b}[31msome high level thing failed\u{1b}[0m
\u{1b}[2m~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~\u{1b}[0m

"
    );
    Ok(())
}
#[test]
fn json_test() -> Result<()> {
    let err = more_anyhow_context().unwrap_err();

    snapshot!(
        format!("{}", to_json(&err)?),
        r#"
{
  "display": "Root cause that used anyhow::bail!()\
",
  "debug": "\
Error chain and added context:\
\
[0]: more context\
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~\
[1]: some stuff\
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~\
[2]: some high level thing failed\
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~\
"
}
"#
    );
    Ok(())
}
