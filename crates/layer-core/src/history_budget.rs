//! Shared accounting for admission and eviction. The current document owns its
//! backing independently; history is charged for additional retained ownership.
use super::*;
use std::collections::HashSet;

pub(super) const BYTE_BUDGET: usize = 512 * 1024 * 1024;
pub(super) const ENTRY_BUDGET: usize = 256;

#[derive(Default)]
pub(crate) struct Accounting {
    roots: HashSet<u64>,
    fields: HashSet<u64>,
    tiles: HashSet<u64>,
    sources: color::source::SourceAccounting,
    selections: HashSet<usize>,
    meshes: HashSet<usize>,
    mesh_arrays: HashSet<usize>,
    resources: HashSet<usize>,
    programs: HashSet<usize>,
    codes: HashSet<usize>,
    extensions: HashSet<usize>,
    backings: HashSet<u64>,
    opaque_resources: HashSet<usize>,
    vector_chunks: HashSet<usize>,
    vector_parts: HashSet<usize>,
}
impl Accounting {
    pub fn new(document: &Document) -> Self {
        Self::seed(document,false)
    }
    pub fn for_admission(document: &Document) -> Self {
        Self::seed(document,true)
    }
    fn seed(document:&Document,authored_only:bool)->Self {
        let mut result = Self::default();
        let mut roots = RootInventory {authored_only,..Default::default()};
        roots.document(document);
        result.charge_inventory(roots);
        result
    }

    pub fn charge(&mut self, entry: &HistoryEntry) -> usize {
        let mut inventory = RootInventory::default(); entry.edit.roots(&mut inventory);
        entry.metadata_bytes.saturating_add(self.charge_inventory(inventory))
    }

    fn charge_inventory(&mut self, inventory: RootInventory<'_>) -> usize {
        let mut bytes = 0usize;
        for state in inventory.rasters.iter().filter_map(|r| r.liquify()) {
            state.field.resources(&mut |id, size| { if self.fields.insert(id) { bytes = bytes.saturating_add(size); } });
            let mut original = RootInventory::default(); original.paint(&state.original);
            bytes = bytes.saturating_add(self.charge_inventory(original));
        }
        for extensions in inventory.extensions { bytes=bytes.saturating_add(self.charge_extensions(extensions)); }
        for resource in inventory.resources { bytes=bytes.saturating_add(self.charge_resource(resource)); }
        for mesh in inventory.meshes { bytes=bytes.saturating_add(self.charge_mesh(mesh)); }
        for selection in inventory.selections { bytes=bytes.saturating_add(self.charge_selection(selection)); }
        for image in inventory.images {bytes=bytes.saturating_add(self.sources.charge_image(image));}
        for object in inventory.vectors {bytes=bytes.saturating_add(self.charge_vector(object));}
        for source in inventory.sources { bytes=bytes.saturating_add(self.sources.charge(source)); }
        for profile in inventory.profiles { bytes=bytes.saturating_add(self.charge_profile(profile)); }
        for program in inventory.programs { bytes=bytes.saturating_add(self.charge_program(program)); }
        for revision in inventory.rasters {
            if !self.roots.insert(revision.identity()) {
                continue;
            }
            match revision.try_data() {
                Some(Ok(data)) => {
                    bytes = bytes.saturating_add(data.tiles.len().saturating_mul(96));
                    for tile in data.tiles.values() {
                        if self.tiles.insert(tile.identity()) {
                            bytes = bytes.saturating_add(match tile.try_backing() {
                                Some(Ok(blob)) => blob.resident_bytes(),
                                _ => {
                                    raster::TileBlob::max_compressed_len(tile.descriptor())
                                        .unwrap_or(raster::MAX_COMPRESSED_TILE_BYTES)
                                }
                            });
                        }
                    }
                }
                // Pending producers reserve the permitted capture, never zero.
                None => bytes = bytes.saturating_add(revision.pending_bytes()),
                Some(Err(_)) => (),
            }
        }
        bytes
    }

    pub(super) fn charge_extension_metadata(&mut self, extensions:&Arc<authored::Extensions>)->usize {
        if !self.extensions.insert(Arc::as_ptr(extensions) as usize) {return 0;}
        extensions.resources.values().fold(crate::extension_record_metadata(extensions),|bytes,resource|{
            bytes.saturating_add(if self.opaque_resources.insert(Arc::as_ptr(resource) as usize){crate::opaque_resource_metadata(resource)}else{0})
        })
    }
    pub(super) fn charge_extensions(&mut self, extensions:&Arc<authored::Extensions>)->usize {
        extensions.resources.values().fold(self.charge_extension_metadata(extensions),|bytes,resource|{
            bytes.saturating_add(if self.backings.insert(resource.backing.identity()){resource.backing.resident_bytes()}else{0})
        })
    }
    pub(super) fn charge_profile(&mut self, profile: &color::ColorProfile) -> usize {
        self.sources.charge_profile(profile)
    }
    fn charge_shader(&mut self,shader:&EffectShader)->usize {
        match shader {
            EffectShader::Code(code)=>if self.codes.insert(code.as_ptr() as usize){code.len()}else{0},
            EffectShader::Linked{sources}=>sources.iter().map(|code|if self.codes.insert(code.as_ptr() as usize){code.len()}else{0}).fold(0usize,usize::saturating_add),
            EffectShader::Modules(modules)=>modules.iter().map(|code|if self.codes.insert(code.as_ptr() as usize){code.len()}else{0}).fold(0usize,usize::saturating_add),
        }
    }
    pub(super) fn charge_program(&mut self, program:&Arc<EffectProgram>)->usize {
        if !self.programs.insert(Arc::as_ptr(program) as usize){return 0;}
        let metadata=std::mem::size_of::<EffectProgram>()+program.id.len()+program.entry.len()
            +json_len(&program.label).saturating_mul(4)+json_len(&program.parameters).saturating_mul(4)
            +json_len(&program.pages).saturating_mul(4)+json_len(&program.constraints).saturating_mul(4)
            +json_len(&program.passes).saturating_mul(4)+json_len(&program.auxiliary).saturating_mul(4);
        let mut bytes=metadata.saturating_add(self.charge_shader(&program.wgsl));
        for lookup in program.lookups.iter(){bytes=bytes.saturating_add(std::mem::size_of::<EffectLookup>()+lookup.entry.len()+lookup.dependencies.iter().map(|key|key.len()).sum::<usize>()).saturating_add(self.charge_shader(&lookup.wgsl));}
        bytes
    }
    pub(super) fn charge_resource(&mut self, resource: &Lut3d) -> usize {
        resource.storage().filter(|bytes| self.resources.insert(bytes.as_ptr() as usize)).map_or(0, |bytes| bytes.len())
    }
    pub(super) fn charge_mesh(&mut self,mesh:&Arc<MeshMap>)->usize {
        let mut bytes=if self.meshes.insert(Arc::as_ptr(mesh) as usize){std::mem::size_of::<MeshMap>()}else{0};
        if self.mesh_arrays.insert(mesh.net.as_ptr() as usize){bytes=bytes.saturating_add(mesh.net.len()*std::mem::size_of::<Point>());}
        for b in &mesh.breakpoints{if self.mesh_arrays.insert(b.as_ptr() as usize){bytes=bytes.saturating_add(b.len()*std::mem::size_of::<f32>());}}
        bytes
    }
    pub(crate) fn charge_vector(&mut self, object: &authored::vector::VectorObject) -> usize {
        use authored::vector::{PaintEntry, Record, Anchor};
        let mut bytes = 0usize;
        for chunk in object.chunks() {
            if !self.vector_chunks.insert(chunk.owner_identity()) { continue; }
            bytes = bytes.saturating_add(128 + std::mem::size_of_val(chunk.records()));
            for record in chunk.records() {
                match record {
                    Record::Path(path) => {
                        if self.vector_parts.insert(Arc::as_ptr(&path.geometry) as usize) {
                            bytes = bytes.saturating_add(64 + std::mem::size_of_val(path.geometry.subpaths()));
                            for run in path.geometry.subpaths().iter().flat_map(|s| s.runs()) {
                                if self.vector_parts.insert(run.as_ptr() as usize) { bytes = bytes.saturating_add(run.len() * std::mem::size_of::<Anchor>()); }
                            }
                        }
                        if self.vector_parts.insert(Arc::as_ptr(&path.style) as usize) {
                            bytes = bytes.saturating_add(path.style.entries.iter().map(|e| std::mem::size_of::<PaintEntry>() + match e {
                                PaintEntry::Stroke(s) => s.profile.as_ref().map_or(0, |p| p.knots().len() * 32),
                                PaintEntry::Opaque(o) => o.properties.len(),
                                PaintEntry::Fill(_) => 0,
                            }).sum::<usize>());
                        }
                    }
                    Record::Opaque(opaque) => bytes = bytes.saturating_add(opaque.properties.len()),
                    Record::End => {}
                }
            }
        }
        bytes
    }
    pub(super) fn charge_selection(&mut self, selection: &Selection) -> usize {
        match &selection.shape {
            SelectionShape::Pixels(pixels) => {
                if self.selections.insert(pixels.words().as_ptr() as usize) {
                    pixels.words().len().saturating_mul(4)
                } else { 0 }
            }
            SelectionShape::Contours(paths) => paths.iter().map(|path| {
                if self.selections.insert(path.as_ptr() as usize) {
                    path.len().saturating_mul(std::mem::size_of::<Point>())
                } else { 0 }
            }).fold(0usize, usize::saturating_add),
        }
    }
}

#[cfg(test)]
mod tests;
