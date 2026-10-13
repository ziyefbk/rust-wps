// Test transport for the real public preview API. Compile against the same
// freshly built library as the candidate CLI; never use a copied implementation.
// The dedicated corpus checker records this source and both binary identities.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = args.next().ok_or("Missing PDF path")?;
    if args.next().is_some() {
        return Err("Unexpected extra arguments".into());
    }
    let report = rust_wps::inspect_for_preview(std::path::Path::new(&path))?;
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}
