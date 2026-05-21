pub fn load(
    path: String,
) -> Result<
    (
        gltf::Document,
        Vec<gltf::buffer::Data>,
        Vec<gltf::image::Data>,
    ),
    Box<dyn std::error::Error>,
> {
    let gltf = gltf::import(path)?;
    Ok(gltf)
}
