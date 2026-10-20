//! Shared accounting for admission and eviction. The current document owns its
//! backing independently; history is charged for additional retained ownership.
use super::*;
use authored::vector::{IdBuild, VectorChunk, VectorObject, WeakVectorObject};
use std::collections::{HashMap, HashSet};

pub(super) const BYTE_BUDGET: usize = 512 * 1024 * 1024;
pub(super) const ENTRY_BUDGET: usize = 256;

#[derive(Debug, Default)]
pub(crate) struct VectorLedger {
    objects: Vec<VectorObject>,
    chunks: HashMap<usize, (authored::Resource<VectorChunk>, u32), IdBuild>,
    parts: HashMap<usize, u32, IdBuild>,
    pub(crate) walked: usize,
}
impl VectorLedger {
    pub(crate) fn sync(&mut self, document: &Document) {
        let mut current: Vec<VectorObject> = Vec::new();
        for (_, _, object) in document.artwork.vectors.iter() { if !current.iter().any(|known| known.same_root(object)) { current.push(object.clone()); } }
        let added: Vec<VectorObject> = current.iter().filter(|object| !self.objects.iter().any(|known| known.same_root(object))).cloned().collect();
        let removed: Vec<VectorObject> = self.objects.iter().filter(|known| !current.iter().any(|object| object.same_root(known))).cloned().collect();
        for chunk in added.iter().flat_map(VectorObject::chunks) {
            let held = self.chunks.entry(chunk.owner_identity()).or_insert_with(|| (chunk.clone(), 0));
            held.1 += 1;
            if held.1 > 1 { continue; }
            self.walked += 1;
            for (part, _) in chunk.footprint().parts.iter() { *self.parts.entry(*part).or_default() += 1; }
        }
        for chunk in removed.iter().flat_map(VectorObject::chunks) {
            let Some(held) = self.chunks.get_mut(&chunk.owner_identity()) else { continue };
            held.1 -= 1;
            if held.1 > 0 { continue; }
            self.chunks.remove(&chunk.owner_identity());
            self.walked += 1;
            for (part, _) in chunk.footprint().parts.iter() {
                if let Some(count) = self.parts.get_mut(part) { *count -= 1; if *count == 0 { self.parts.remove(part); } }
            }
        }
        self.objects = current;
    }
}

#[derive(Debug)]
pub(crate) struct VectorLink { held: usize, adjacent: Option<WeakVectorObject>, released: Vec<authored::Resource<VectorChunk>> }
impl VectorLink {
    pub(crate) fn of(edit: &Edit, document: &Document) -> Vec<Self> {
        fn changes<'a>(edit: &'a Edit, out: &mut Vec<(authored::VectorHandle, &'a VectorObject)>) {
            match edit {
                Edit::Vector(change) => out.extend(change.value.as_ref().map(|object| (change.handle, object))),
                Edit::Batch(edits) => for edit in edits { changes(edit, out); },
                _ => {}
            }
        }
        let mut held = Vec::new();
        changes(edit, &mut held);
        held.iter().filter(|(handle, _)| held.iter().filter(|(other, _)| other == handle).count() == 1).map(|&(handle, object)| {
            let adjacent = document.artwork.vectors.get(handle);
            let kept: HashSet<usize, IdBuild> = adjacent.iter().flat_map(|adjacent| adjacent.chunks().iter().map(|chunk| chunk.owner_identity())).collect();
            Self { held: object.identity(), adjacent: adjacent.map(VectorObject::downgrade), released: object.chunks().iter().filter(|chunk| !kept.contains(&chunk.owner_identity())).cloned().collect() }
        }).collect()
    }
}

#[derive(Default)]
pub(crate) struct Accounting<'a> {
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
    vector_chunks: HashSet<usize, IdBuild>,
    vector_parts: HashSet<usize, IdBuild>,
    vector_roots: HashSet<usize, IdBuild>,
    document: Option<&'a VectorLedger>,
    pub(crate) walked: usize,
}
impl<'a> Accounting<'a> {
    pub fn new(document: &Document) -> Self {
        Self::seed(document,false,None)
    }
    pub(crate) fn with_ledger(document: &Document, ledger: &'a VectorLedger, authored_only: bool) -> Self {
        Self::seed(document,authored_only,Some(ledger))
    }
    fn seed(document:&Document,authored_only:bool,ledger:Option<&'a VectorLedger>)->Self {
        let mut result = Self { document: ledger, ..Self::default() };
        let mut roots = RootInventory {authored_only,..Default::default()};
        roots.document(document);
        if let Some(ledger) = ledger {
            roots.vectors.clear();
            result.vector_roots.extend(ledger.objects.iter().map(VectorObject::identity));
        }
        result.charge_inventory(roots, &[]);
        result
    }

    pub fn charge(&mut self, entry: &HistoryEntry) -> usize {
        let mut inventory = RootInventory::default(); entry.edit.roots(&mut inventory);
        entry.metadata_bytes.saturating_add(self.charge_inventory(inventory, &entry.vectors))
    }

    fn charge_inventory(&mut self, inventory: RootInventory<'_>, links: &[VectorLink]) -> usize {
        let mut bytes = 0usize;
        for state in inventory.rasters.iter().filter_map(|r| r.liquify()) {
            state.field.resources(&mut |id, size| { if self.fields.insert(id) { bytes = bytes.saturating_add(size); } });
            let mut original = RootInventory::default(); original.paint(&state.original);
            bytes = bytes.saturating_add(self.charge_inventory(original, &[]));
        }
        for extensions in inventory.extensions { bytes=bytes.saturating_add(self.charge_extensions(extensions)); }
        for resource in inventory.resources { bytes=bytes.saturating_add(self.charge_resource(resource)); }
        for mesh in inventory.meshes { bytes=bytes.saturating_add(self.charge_mesh(mesh)); }
        for selection in inventory.selections { bytes=bytes.saturating_add(self.charge_selection(selection)); }
        for image in inventory.images {bytes=bytes.saturating_add(self.sources.charge_image(image));}
        for object in inventory.vectors {bytes=bytes.saturating_add(self.charge_object(object, links));}
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
    pub(crate) fn charge_vector(&mut self, object: &VectorObject) -> usize { self.charge_object(object, &[]) }
    fn charge_object(&mut self, object: &VectorObject, links: &[VectorLink]) -> usize {
        if self.vector_roots.contains(&object.identity()) { return 0; }
        let roots = &self.vector_roots;
        let released = links.iter().find(|link| link.held == object.identity())
            .filter(|link| link.adjacent.as_ref().is_none_or(|adjacent| adjacent.identity().is_some_and(|root| roots.contains(&root))))
            .map(|link| link.released.as_slice());
        self.vector_roots.insert(object.identity());
        released.unwrap_or(object.chunks()).iter().fold(0usize, |bytes, chunk| bytes.saturating_add(self.charge_chunk(chunk)))
    }
    fn charge_chunk(&mut self, chunk: &authored::Resource<VectorChunk>) -> usize {
        if self.document.is_some_and(|ledger| ledger.chunks.contains_key(&chunk.owner_identity())) || !self.vector_chunks.insert(chunk.owner_identity()) { return 0; }
        self.walked += 1;
        let footprint = chunk.footprint();
        let document = self.document;
        footprint.parts.iter().fold(footprint.bytes, |bytes, (part, size)| {
            let new = !document.is_some_and(|ledger| ledger.parts.contains_key(part)) && self.vector_parts.insert(*part);
            if new { bytes.saturating_add(*size) } else { bytes }
        })
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
