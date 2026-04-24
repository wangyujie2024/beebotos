use std::io;

fn main() -> io::Result<()> {
    // Tell cargo to rerun if proto files change
    println!("cargo:rerun-if-changed=../../proto/skills/registry.proto");
    println!("cargo:rerun-if-changed=../../proto/common/types.proto");

    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .compile(
            &[
                "../../proto/skills/registry.proto",
                "../../proto/common/types.proto",
            ],
            &["../../proto"],
        )
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    Ok(())
}
