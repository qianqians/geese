use gltf::Gltf;

fn load(path:String) -> Result<Gltf, Box<dyn std::error::Error>> {
    let gltf = Gltf::open(path)?;
    Ok(gltf)
}