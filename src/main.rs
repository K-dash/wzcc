use anyhow::Result;
use wzcc::ui::App;

fn main() -> Result<()> {
    let mut app = App::new();
    app.run()?;

    Ok(())
}
