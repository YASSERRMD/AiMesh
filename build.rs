use std::io::Result;

fn main() -> Result<()> {
    // Tell Cargo to re-run this build script if the proto file changes
    println!("cargo:rerun-if-changed=proto/message.proto");
    
    // Compile the protobuf definitions
    prost_build::compile_protos(&["proto/message.proto"], &["proto/"])?;
    
    Ok(())
}
