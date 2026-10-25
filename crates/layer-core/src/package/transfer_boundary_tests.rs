fn boundary_id(value: u8) -> PortableId { let mut bytes = [0; 16]; bytes[15] = value; PortableId::from_bytes(bytes) }

fn boundary_init_descriptor(lengths: [usize; 3]) -> TransferDescriptor {
    let manifest: Value = serde_json::from_str(include_str!("../../tests/fixtures/capy/empty.json")).unwrap();
    let document = serde_json::from_value(manifest["document"].clone()).unwrap();
    let layout = TransferLayout {compositions:vec![boundary_id(1)],stacks:vec![boundary_id(2)],outputs:vec![boundary_id(5)],
        occurrences:vec![],paint:vec![],coverage:vec![],effects:vec![],objects:vec![],vectors:vec![],selections:vec![],guides:vec![]};
    let private_resources = (0..2).map(|index| {
        let id = boundary_id(110 + index as u8);
        (id, (json!({"id":id,"type":"future.bytes/1","data":{},"encoding":"future.raw/1",
            "bytes":lengths[index].to_string(),"crc32":"00000000"}),index))
    }).collect();
    TransferDescriptor {manifest,layout,checkpoint:CaptureCheckpoint {owner:1,document,session_generation:1,
        artwork_generation:1,working_generation:1,edit_checkpoint:1},working_selection:None,
        lengths:lengths.map(|length|length as u64).to_vec(),resources:BTreeMap::new(),
        selections:vec![SelectionTransfer {extent:[8,1],bounds:[0,0,8,1],bytes:false,chunks:vec![boundary_id(120)],payload:2}],
        retained_resources:BTreeMap::new(),private_resources}
}
fn boundary_init_receiver(lengths: [usize; 3]) -> TransferReceiver {
    TransferReceiver::new(boundary_init_descriptor(lengths),ProjectLimits::default()).unwrap()
}
fn boundary_parts(length: usize, unit: usize, mask: usize) -> Vec<std::ops::Range<usize>> {
    let mut ranges=Vec::new(); let mut start=0;
    for end in (unit..length).step_by(unit) {
        if mask & (1 << (end/unit-1)) != 0 { ranges.push(start..end); start=end; }
    }
    ranges.push(start..length); ranges
}
fn boundary_assert_payloads(prepared: &PreparedTransfer, expected: &[&[u8]]) {
    for (index,bytes) in expected.iter().enumerate() {
        assert_eq!(prepared.read_chunk(index,0,bytes.len()).unwrap(),*bytes);
    }
}

#[test]
fn transfer_boundary_init_all_small_partitions_and_interleaving() {
    let a=[3,1,4,1,5,9,2]; let b=[6,5,3,5,8];
    let words=[0x01234567u32,0x89abcdef,0xfedcba98];
    let word_bytes:Vec<u8>=words.into_iter().flat_map(u32::to_le_bytes).collect();
    for byte_mask in 0..64 { for word_mask in 0..4 {
        let mut receiver=boundary_init_receiver([a.len(),b.len(),word_bytes.len()]);
        let a_parts=boundary_parts(a.len(),1,byte_mask);
        let b_parts=boundary_parts(b.len(),1,byte_mask & 15);
        let word_parts=boundary_parts(word_bytes.len(),4,word_mask);
        for step in 0..a_parts.len().max(b_parts.len()).max(word_parts.len()) {
            receiver.push_chunk(0,&[]).unwrap(); receiver.push_chunk(2,&[]).unwrap();
            if let Some(range)=word_parts.get(step) {receiver.push_chunk(2,&word_bytes[range.clone()]).unwrap();}
            if let Some(range)=b_parts.get(step) {receiver.push_chunk(1,&b[range.clone()]).unwrap();}
            if let Some(range)=a_parts.get(step) {receiver.push_chunk(0,&a[range.clone()]).unwrap();}
        }
        assert!(receiver.missing_payloads().is_empty());
        boundary_assert_payloads(&receiver.finish().unwrap(),&[&a,&b,&word_bytes]);
    }}
}
#[test]
fn transfer_boundary_init_every_incomplete_prefix_is_rejected() {
    let bytes=[1,2,3,4,5,6,7];
    for prefix in 0..bytes.len() {
        let mut receiver=boundary_init_receiver([bytes.len(),0,0]);
        receiver.push_chunk(0,&bytes[..prefix]).unwrap();
        assert!(receiver.finish().is_err(),"byte prefix {prefix}");
    }
    let words=[1u32,2,3].into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
    for prefix in (0..words.len()).step_by(4) {
        let mut receiver=boundary_init_receiver([0,0,words.len()]);
        receiver.push_chunk(2,&words[..prefix]).unwrap();
        assert!(receiver.finish().is_err(),"word prefix {prefix}");
    }
    assert!(boundary_init_receiver([0,0,0]).finish().is_ok());
}
#[test]
fn transfer_boundary_init_rejected_chunks_do_not_advance_positions() {
    let mut receiver=boundary_init_receiver([3,0,8]);
    assert!(receiver.push_chunk(3,&[1]).is_err());
    assert!(receiver.push_chunk(0,&[1,2,3,4]).is_err());
    assert_eq!(receiver.positions,[0,0,0]);
    for size in [1,2,3,5,6,7] {
        assert!(receiver.push_chunk(2,&vec![0;size]).is_err());
        assert_eq!(receiver.positions,[0,0,0]);
    }
    receiver.push_chunk(0,&[1,2,3]).unwrap();
    receiver.push_chunk(2,&1u32.to_le_bytes()).unwrap();
    assert!(receiver.push_chunk(2,&[0,0]).is_err());
    assert_eq!(receiver.positions,[3,0,4]);
    receiver.push_chunk(2,&2u32.to_le_bytes()).unwrap();
    assert!(receiver.push_chunk(0,&[4]).is_err());
    assert!(receiver.push_chunk(2,&3u32.to_le_bytes()).is_err());
    let expected=[1u32,2].into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
    boundary_assert_payloads(&receiver.finish().unwrap(),&[&[1,2,3],&[],&expected]);
}
#[test]
fn transfer_boundary_init_reuse_is_complete_immutable_and_length_bound() {
    let descriptor=boundary_init_descriptor([3,2,4]);
    let mut receiver=TransferReceiver::new(descriptor.clone(),ProjectLimits::default()).unwrap();
    receiver.push_chunk(0,&[1,2,3]).unwrap(); receiver.push_chunk(1,&[4,5]).unwrap();
    receiver.push_chunk(2,&0x12345678u32.to_le_bytes()).unwrap();
    let previous=receiver.finish().unwrap();
    let mut reused=TransferReceiver::new_reusing(descriptor.clone(),ProjectLimits::default(),&previous).unwrap();
    assert!(reused.missing_payloads().is_empty());
    for index in 0..3 {assert!(reused.push_chunk(index,&[]).is_err());}
    let complete=reused.finish().unwrap();
    for index in 0..3 {assert_eq!(previous.read_chunk(index,0,descriptor.lengths[index] as usize).unwrap(),complete.read_chunk(index,0,descriptor.lengths[index] as usize).unwrap());}
    let mut changed=descriptor; changed.lengths[0]=4;
    changed.private_resources.get_mut(&boundary_id(110)).unwrap().0["bytes"]="4".into();
    let receiver=TransferReceiver::new_reusing(changed,ProjectLimits::default(),&previous).unwrap();
    assert_eq!(receiver.missing_payloads(),vec![0]); assert!(receiver.finish().is_err());
}

fn boundary_wire(prepared:&PreparedTransfer)->(TransferDescriptor,Vec<Vec<u8>>) {
    let descriptor=serde_json::from_slice(&serde_json::to_vec(prepared.descriptor()).unwrap()).unwrap();
    let payloads=(0..prepared.payload_count()).map(|index| {
        let length=prepared.payload_len(index).unwrap() as usize;
        assert!(length<=MAX_RANGE_BYTES);
        prepared.read_chunk(index,0,length).unwrap()
    }).collect();
    (descriptor,payloads)
}
fn boundary_receive(descriptor:TransferDescriptor,payloads:Vec<Vec<u8>>)->Result<PreparedTransfer,String> {
    let mut receiver=TransferReceiver::new(descriptor,ProjectLimits::default())?;
    for (index,payload) in payloads.iter().enumerate() {receiver.push_chunk(index,payload)?;}
    receiver.finish()
}
fn boundary_adopt(descriptor:TransferDescriptor,payloads:Vec<Vec<u8>>)->Result<(ArtworkCapture,Option<Selection>),String> {
    boundary_receive(descriptor,payloads)?.adopt_verified_with_selection(ProjectLimits::default(),&AtomicBool::new(false))
}
fn boundary_valid_wire(prepared:&PreparedTransfer)->(TransferDescriptor,Vec<Vec<u8>>) {
    let (descriptor,payloads)=boundary_wire(prepared);
    assert!(boundary_adopt(descriptor.clone(),payloads.clone()).is_ok(),"valid wire control must adopt");
    (descriptor,payloads)
}
fn boundary_resource_record(descriptor:&mut TransferDescriptor,id:PortableId)->&mut Value {
    descriptor.manifest["resources"].as_array_mut().unwrap().iter_mut().find(|record|record["id"]==json!(id)).unwrap()
}
fn boundary_replace_payload(descriptor:&mut TransferDescriptor,payloads:&mut [Vec<u8>],id:PortableId,index:usize,bytes:Vec<u8>) {
    descriptor.lengths[index]=bytes.len() as u64;
    boundary_resource_record(descriptor,id)["bytes"]=bytes.len().to_string().into();
    payloads[index]=bytes;
}
fn boundary_replace_tile(descriptor:&mut TransferDescriptor,payloads:&mut [Vec<u8>],id:PortableId,index:usize,bytes:Vec<u8>) {
    use sha2::{Digest,Sha256};
    let pixels=resources::parse_descriptor(&boundary_resource_record(descriptor,id)["data"]).unwrap();
    let mut hash=Sha256::new();hash.update(serde_json::to_vec(&pixels).unwrap());hash.update(&bytes);
    let VerifiedResource::Tile{fingerprint,..}=descriptor.resources.get_mut(&id).unwrap()else{unreachable!()};
    *fingerprint=Some(hash.finalize().into());
    boundary_replace_payload(descriptor,payloads,id,index,bytes);
}
fn boundary_lut_transfer()->PreparedTransfer {
    let cancel=AtomicBool::new(false); let mut original=capture(); let art=Arc::make_mut(&mut original.artwork);
    let lookup=Arc::new(Lut3d::from_samples(2,[[0.;3],[1.;3]],Arc::from("lookup"),vec![[0.25;3];8].into()).unwrap());
    let program=crate::bundled_effect_catalog().get("color_lookup").unwrap().program();
    let mut effect=EffectInstance::new(program.clone()); effect.set("resource",EffectValue::Lut3d(Some(lookup))).unwrap();
    let application=art.effects.insert(boundary_id(130),EffectApplication::new(program,effect.values,[19,11])).unwrap();
    let occurrence=art.occurrences.insert(boundary_id(131),Occurrence::new(OccurrenceContent::Effect(application),"lookup")).unwrap();
    let stack=art.compositions.get(art.root).unwrap().result; art.stacks.get_mut(stack).unwrap().entries.insert(0,occurrence);
    PreparedTransfer::capture(&original,&cancel).unwrap()
}
#[test]
fn transfer_boundary_wire_lut_nan_digest_and_spaces_are_not_authority() {
    let prepared=boundary_lut_transfer();
    for mutation in 0..3 {
        let (mut descriptor,mut payloads)=boundary_valid_wire(&prepared);
        let state=descriptor.resources.values_mut().find(|state|matches!(state,VerifiedResource::Lut{..})).unwrap();
        let VerifiedResource::Lut{payload,digest,spaces,..}=state else{unreachable!()};
        match mutation {0=>{use sha2::{Digest,Sha256};payloads[*payload][..4].copy_from_slice(&f32::NAN.to_le_bytes());*digest=Sha256::digest(&payloads[*payload]).into();},1=>digest[0]^=1,_=>{assert!(spaces.count_ones()>1);*spaces^=1;}}
        assert!(boundary_adopt(descriptor,payloads).is_err(),"LUT mutation {mutation} reached publication");
    }
}
#[test]
fn transfer_boundary_wire_bad_compression_and_fingerprint_are_rejected() {
    let prepared=PreparedTransfer::capture(&capture(),&AtomicBool::new(false)).unwrap();
    for mutation in 0..2 {
        let (mut descriptor,mut payloads)=boundary_valid_wire(&prepared);
        let (&id,state)=descriptor.resources.iter_mut().find(|(_,state)|matches!(state,VerifiedResource::Tile{..})).unwrap();
        let VerifiedResource::Tile{payload,fingerprint}=state else{unreachable!()}; let index=*payload;
        if mutation==0 {boundary_replace_tile(&mut descriptor,&mut payloads,id,index,vec![0]);}
        else {let mut bad=fingerprint.unwrap();bad[0]^=1;*fingerprint=Some(bad);}
        assert!(boundary_adopt(descriptor,payloads).is_err(),"tile mutation {mutation} reached publication");
    }
}
fn boundary_float_transfer()->PreparedTransfer {
    use crate::color::{SampleDepth,source::SourceImage};
    let mut original=capture(); let art=Arc::make_mut(&mut original.artwork);
    let paint=art.paint.iter().next().unwrap().0;
    let base=art.paint.get_mut(paint).unwrap().base.as_mut().unwrap();
    let mut interpretation=base.image.interpretation.clone(); interpretation.depth=SampleDepth::F32;
    let descriptor=interpretation.descriptor();
    let compressed=boundary_float_compressed(false);
    let tile=Arc::new(crate::raster::TileBlob::from_package(boundary_id(140),descriptor,compressed.into()).unwrap());
    let source=SourceImage {extent:[19,11],resolution:None,interpretation,tiles:[([0,0],tile)].into()};
    base.image=Image::with_id(base.image.id(),Arc::new(source));
    PreparedTransfer::capture(&original,&AtomicBool::new(false)).unwrap()
}
fn boundary_float_compressed(nan:bool)->Vec<u8> {
    let pixel=[if nan {f32::NAN}else{0.25},0.25,0.25,1.];
    let pixels=crate::raster::TILE_SIZE as usize*crate::raster::TILE_SIZE as usize;
    let mut shuffled=Vec::with_capacity(pixels*16);
    for value in pixel {for byte in value.to_le_bytes() {shuffled.extend(std::iter::repeat_n(byte,pixels));}}
    lz4_flex::block::compress(&shuffled)
}
#[test]
fn transfer_boundary_wire_nonfinite_tile_samples_are_rejected() {
    let prepared=boundary_float_transfer();let (mut descriptor,mut payloads)=boundary_valid_wire(&prepared);
    let (&id,state)=descriptor.resources.iter().find(|(_,state)|matches!(state,VerifiedResource::Tile{..})).unwrap();
    let VerifiedResource::Tile{payload,..}=state else{unreachable!()};let index=*payload;
    boundary_replace_tile(&mut descriptor,&mut payloads,id,index,boundary_float_compressed(true));
    assert!(boundary_adopt(descriptor,payloads).is_err(),"nonfinite samples reached publication");
}
fn boundary_selection_transfer(cached:bool)->PreparedTransfer {
    let pixels=Arc::new(SelectionPixels::new([9,1],[0,0,9,1],vec![0x44444444,4]).unwrap());
    if cached {pixels.package_chunks().unwrap();}
    PreparedTransfer::capture_with_selection(&capture(),&Some(Selection::pixels(pixels)),&AtomicBool::new(false)).unwrap()
}
#[test]
fn transfer_boundary_wire_selection_nibbles_and_padding_are_rejected() {
    let prepared=boundary_selection_transfer(false);
    for mutation in 0..2 {
        let (descriptor,mut payloads)=boundary_valid_wire(&prepared);let payload=descriptor.selections[0].payload;
        if mutation==0 {payloads[payload][..4].copy_from_slice(&0x44444445u32.to_le_bytes());}
        else {payloads[payload][4..8].copy_from_slice(&0x44u32.to_le_bytes());}
        assert!(boundary_adopt(descriptor,payloads).is_err(),"selection mutation {mutation} reached publication");
    }
}
#[test]
fn transfer_boundary_wire_selection_bounds_cannot_omit_nonzero_coverage() {
    let prepared=boundary_selection_transfer(false);let (mut descriptor,payloads)=boundary_valid_wire(&prepared);
    descriptor.selections[0].bounds=[0,0,8,1];
    descriptor.working_selection.as_mut().unwrap()["shape"]["pixels"]["bounds"]=json!([0,0,8,1]);
    let chunks=descriptor.selections[0].chunks.clone();
    for id in chunks {boundary_resource_record(&mut descriptor,id)["data"]["bounds"]=json!([0,0,8,1]);}
    assert!(boundary_adopt(descriptor,payloads).is_err(),"coverage outside declared bounds reached publication");
}
#[test]
fn transfer_boundary_wire_selection_cached_chunks_cannot_disagree_with_words() {
    let prepared=boundary_selection_transfer(true);let (descriptor,mut payloads)=boundary_valid_wire(&prepared);
    let index=descriptor.selections[0].payload;
    payloads[index][..4].copy_from_slice(&0x33333333u32.to_le_bytes());
    let Ok((_,selection))=boundary_adopt(descriptor,payloads) else{return};
    let SelectionShape::Pixels(pixels)=selection.unwrap().shape else{panic!("pixel selection")};
    let chunks=pixels.package_chunks().unwrap().to_vec();
    let decoded=SelectionPixels::from_package_chunks(pixels.extent(),pixels.bounds(),false,chunks).unwrap();
    assert_eq!(decoded.transfer_words(),pixels.transfer_words(),"untrusted cached chunks reached publication");
}
fn boundary_cached_metadata_transfer()->PreparedTransfer {
    let mut original=capture();let art=Arc::make_mut(&mut original.artwork);
    let raw:Arc<[u8]>=vec![b'A';64].into();let encoded=lz4_flex::block::compress(&raw).into();
    art.metadata=Arc::new(crate::PhotoMetadata {xmp:Some(Resource::with_id_and_encoded(boundary_id(150),raw,
        EncodedBytes{encoding:ResourceEncoding::Lz4,bytes:encoded})),..Default::default()});
    PreparedTransfer::capture(&original,&AtomicBool::new(false)).unwrap()
}
#[test]
fn transfer_boundary_wire_poisoned_encoded_cache_is_rejected_or_discarded() {
    let prepared=boundary_cached_metadata_transfer();let (mut descriptor,mut payloads)=boundary_valid_wire(&prepared);
    let VerifiedResource::Bytes{encoded:Some(encoded),..}=&descriptor.resources[&boundary_id(150)]else{panic!("encoded bytes")};let index=encoded.payload;
    payloads[index]=lz4_flex::block::compress(&[b'B';64]);descriptor.lengths[index]=payloads[index].len() as u64;
    let Ok((loaded,_))=boundary_adopt(descriptor,payloads) else{return};
    let resource=loaded.artwork.metadata.xmp.as_ref().unwrap(); assert_eq!(&**resource,&[b'A';64]);
    if let Some(encoded)=resource.encoded_if_ready() {
        let decoded=match encoded.encoding {ResourceEncoding::Raw=>encoded.bytes.to_vec(),ResourceEncoding::Lz4=>lz4_flex::block::decompress(&encoded.bytes,resource.len()).unwrap()};
        assert_eq!(decoded,resource.to_vec(),"poisoned encoded cache reached publication");
    }
}
#[test]
fn transfer_boundary_wire_opaque_crc_is_verified_before_publication() {
    let mut original=capture();let art=Arc::make_mut(&mut original.artwork);let bytes:Arc<[u8]>=vec![17;32].into();let id=boundary_id(160);
    let resource=Arc::new(OpaqueResource{id,kind:"future.bytes/1".into(),data:json!({}),encoding:"future.raw/1".into(),extra_fields:Default::default(),
        backing:ImmutableBacking::new(Arc::new(bytes.clone())).unwrap(),offset:0,length:bytes.len() as u64,crc32:crc32fast::hash(&bytes)});
    let extensions=Arc::make_mut(&mut art.extensions);extensions.resources.insert(id,resource);
    let record=boundary_id(161);extensions.records.insert(record,json!({"id":record,"type":"future.attachment/1","ancillary":true,"copy_safe":true,"data":{"payload":resources::reference(id)}}));
    let prepared=PreparedTransfer::capture(&original,&AtomicBool::new(false)).unwrap();let (mut descriptor,payloads)=boundary_valid_wire(&prepared);
    boundary_resource_record(&mut descriptor,id)["crc32"]=format!("{:08x}",crc32fast::hash(&bytes)^1).into();
    assert!(boundary_adopt(descriptor,payloads).is_err(),"unchecked opaque CRC reached publication");
}

fn boundary_reuse_receive(previous:&PreparedTransfer,descriptor:TransferDescriptor,payloads:&[Vec<u8>])->PreparedTransfer {
    let mut receiver=TransferReceiver::new_reusing(descriptor,ProjectLimits::default(),previous).unwrap();
    for index in receiver.missing_payloads() {receiver.push_chunk(index,&payloads[index]).unwrap();}
    receiver.finish().unwrap()
}

#[test]
fn transfer_boundary_reuse_selection_words_do_not_approve_new_cached_chunks() {
    let cancel=AtomicBool::new(false);let prepared=boundary_selection_transfer(true);
    let (descriptor,payloads)=boundary_valid_wire(&prepared);
    let previous=boundary_receive(descriptor.clone(),payloads.clone()).unwrap();
    previous.validate_payloads(ProjectLimits::default(),&cancel).unwrap();
    assert!(boundary_reuse_receive(&previous,descriptor.clone(),&payloads).adopt_verified_with_selection(ProjectLimits::default(),&cancel).is_ok());
    let words=descriptor.selections[0].payload;let id=descriptor.selections[0].chunks[0];
    let VerifiedResource::Coverage{payload:Some(chunk)}=descriptor.resources[&id] else{panic!("cached coverage")};
    for malformed in [true,false] {
        let mut changed=descriptor.clone();let mut bytes=payloads.clone();
        let replacement=if malformed {vec![0]} else {
            let other=SelectionPixels::new([9,1],[0,0,9,1],vec![0x33333333,3]).unwrap();
            other.package_chunks().unwrap()[0].to_vec()
        };
        boundary_replace_payload(&mut changed,&mut bytes,id,chunk,replacement);
        boundary_resource_record(&mut changed,id)["crc32"]=format!("{:08x}",crc32fast::hash(&bytes[chunk])).into();
        let receiver=TransferReceiver::new_reusing(changed.clone(),ProjectLimits::default(),&previous).unwrap();
        assert!(!receiver.missing_payloads().contains(&words),"validated selection words must exercise reuse");
        assert_eq!(receiver.missing_payloads(),vec![chunk],"only the changed cached chunk should need transfer");
        let received=boundary_reuse_receive(&previous,changed,&bytes);
        assert!(received.payload_trusted(words));assert!(!received.payload_trusted(chunk));
        let Ok((_,selection))=received.adopt_verified_with_selection(ProjectLimits::default(),&cancel) else{continue};
        let SelectionShape::Pixels(pixels)=selection.unwrap().shape else{panic!("pixel selection")};
        assert_eq!(pixels.transfer_words().as_ref(),&[0x44444444,4]);
        let chunks=pixels.package_chunks().unwrap().to_vec();
        let decoded=SelectionPixels::from_package_chunks(pixels.extent(),pixels.bounds(),false,chunks).unwrap();
        assert_eq!(decoded.transfer_words(),pixels.transfer_words(),"new cache bytes inherited reused word approval");
    }
}

fn boundary_cached_code_transfer()->PreparedTransfer {
    let mut original=capture();let art=Arc::make_mut(&mut original.artwork);
    let mut program=(*crate::effect_catalog::custom_program("solid_color")).clone();
    let mut sources=program.wgsl.sources().unwrap().to_vec();
    let raw=sources[0].storage().clone();let encoded=lz4_flex::block::compress(raw.as_bytes()).into();
    sources[0]=Resource::with_id_and_encoded(boundary_id(170),raw,EncodedBytes{encoding:ResourceEncoding::Lz4,bytes:encoded});
    program.wgsl=crate::EffectShader::Linked{sources:sources.into()};
    let program=Arc::new(program);let instance=EffectInstance::new(program.clone());
    let handle=art.effects.iter().next().unwrap().0;
    *art.effects.get_mut(handle).unwrap()=EffectApplication::new(program,instance.values,[19,11]);
    PreparedTransfer::capture(&original,&AtomicBool::new(false)).unwrap()
}

#[test]
fn transfer_boundary_reuse_typed_code_does_not_approve_new_encoded_cache() {
    let cancel=AtomicBool::new(false);let previous=boundary_cached_code_transfer();
    previous.validate_payloads(ProjectLimits::default(),&cancel).unwrap();
    let (mut descriptor,mut payloads)=boundary_wire(&previous);
    let control=boundary_reuse_receive(&previous,descriptor.clone(),&payloads);
    control.adopt_with_programs(ProjectLimits::default(),&cancel,&previous.trusted_programs).unwrap();
    let VerifiedResource::Code{payload:raw,encoded:Some(encoded)}=descriptor.resources.get_mut(&boundary_id(170)).unwrap() else{panic!("typed code cache")};
    let raw=*raw;let cache=encoded.payload;
    encoded.encoding=ResourceEncoding::Raw;
    payloads[cache]=vec![b'!';payloads[raw].len()];descriptor.lengths[cache]=payloads[cache].len() as u64;
    assert_ne!(payloads[cache],payloads[raw]);
    let receiver=TransferReceiver::new_reusing(descriptor.clone(),ProjectLimits::default(),&previous).unwrap();
    assert_eq!(receiver.missing_payloads(),vec![cache]);
    let received=boundary_reuse_receive(&previous,descriptor,&payloads);
    assert!(matches!(&received.payloads[raw],TransferPayload::Code(_)),"must exercise the retained typed Code arm");
    assert!(received.payload_trusted(raw));assert!(!received.payload_trusted(cache));
    assert!(received.validate_payloads(ProjectLimits::default(),&cancel).is_err(),"ignored cache bytes acquired validation provenance");
    assert!(received.adopt_with_programs(ProjectLimits::default(),&cancel,&previous.trusted_programs).is_err(),"poisoned new cache passed publication");
}

fn boundary_retained_metadata_transfer()->PreparedTransfer {
    let mut original=capture();let art=Arc::make_mut(&mut original.artwork);let id=boundary_id(180);
    let raw:Arc<[u8]>=vec![b'A';64].into();let compressed:Arc<[u8]>=lz4_flex::block::compress(&raw).into();
    art.metadata=Arc::new(crate::PhotoMetadata{xmp:Some(Resource::with_id(id,raw.clone())),..Default::default()});
    let opaque=Arc::new(OpaqueResource{id,kind:"capy.photo-metadata/1".into(),data:json!({"kind":"xmp","decoded_bytes":raw.len().to_string()}),
        encoding:"capy.lz4-bytes/1".into(),extra_fields:Default::default(),backing:ImmutableBacking::new(Arc::new(compressed.clone())).unwrap(),
        offset:0,length:compressed.len() as u64,crc32:crc32fast::hash(&compressed)});
    let extensions=Arc::make_mut(&mut art.extensions);extensions.resources.insert(id,opaque);
    let record=boundary_id(181);extensions.records.insert(record,json!({"id":record,"type":"future.metadata/1","ancillary":true,"copy_safe":true,"data":{"payload":resources::reference(id)}}));
    PreparedTransfer::capture(&original,&AtomicBool::new(false)).unwrap()
}

#[test]
fn transfer_boundary_retained_known_resource_crc_does_not_approve_different_decoded_bytes() {
    let cancel=AtomicBool::new(false);let prepared=boundary_retained_metadata_transfer();
    let (mut descriptor,mut payloads)=boundary_valid_wire(&prepared);let id=boundary_id(180);
    let VerifiedResource::Bytes{payload:raw,encoded:None}=descriptor.resources[&id] else{panic!("canonical raw owner without encoded cache")};
    let retained=descriptor.retained_resources[&id];assert_ne!(raw,retained);
    assert_eq!(payloads[raw],vec![b'A';64]);
    let replacement=lz4_flex::block::compress(&[b'B';64]);
    assert_eq!(lz4_flex::block::decompress(&replacement,64).unwrap(),vec![b'B';64]);
    boundary_replace_payload(&mut descriptor,&mut payloads,id,retained,replacement);
    boundary_resource_record(&mut descriptor,id)["crc32"]=format!("{:08x}",crc32fast::hash(&payloads[retained])).into();
    let received=boundary_receive(descriptor,payloads).unwrap();
    assert!(received.validate_payloads(ProjectLimits::default(),&cancel).is_err(),"valid CRC hid a conflicting retained representation");
    assert!(received.adopt_verified(ProjectLimits::default(),&cancel).is_err(),"retained bytes disagreed with the published canonical resource");
}

fn boundary_dual_cached_metadata_transfer()->PreparedTransfer {
    let mut original=capture();let art=Arc::make_mut(&mut original.artwork);
    let cached=|id,byte| {
        let raw:Arc<[u8]>=vec![byte;64].into();let encoded=lz4_flex::block::compress(&raw).into();
        Resource::with_id_and_encoded(id,raw,EncodedBytes{encoding:ResourceEncoding::Lz4,bytes:encoded})
    };
    art.metadata=Arc::new(crate::PhotoMetadata{xmp:Some(cached(boundary_id(190),b'A')),iptc:Some(cached(boundary_id(191),b'B')),..Default::default()});
    PreparedTransfer::capture(&original,&AtomicBool::new(false)).unwrap()
}

fn boundary_merge_byte_payloads(descriptor:&mut TransferDescriptor,payloads:&mut Vec<Vec<u8>>,keep:usize,remove:usize)->usize {
    assert_ne!(keep,remove);assert_eq!(descriptor.lengths[keep],descriptor.lengths[remove]);
    let remap=|index| {let index=if index==remove {keep}else{index};if index>remove {index-1}else{index}};
    for state in descriptor.resources.values_mut() {
        match state {
            VerifiedResource::Bytes{payload,encoded}|VerifiedResource::Code{payload,encoded}|VerifiedResource::Lut{payload,encoded,..}=>{
                *payload=remap(*payload);if let Some(encoded)=encoded {encoded.payload=remap(encoded.payload);}
            },
            VerifiedResource::Tile{payload,..}|VerifiedResource::Vector{payload}|VerifiedResource::Opaque{payload}=>*payload=remap(*payload),
            VerifiedResource::Coverage{payload}=>{if let Some(payload)=payload {*payload=remap(*payload);}},
        }
    }
    for selection in &mut descriptor.selections {selection.payload=remap(selection.payload);}
    for payload in descriptor.retained_resources.values_mut() {*payload=remap(*payload);}
    for (_,payload) in descriptor.private_resources.values_mut() {*payload=remap(*payload);}
    descriptor.lengths.remove(remove);payloads.remove(remove);remap(keep)
}

#[test]
fn transfer_boundary_reuse_merged_byte_bindings_require_receive_and_check_both_caches() {
    let cancel=AtomicBool::new(false);let previous=boundary_dual_cached_metadata_transfer();
    let (mut descriptor,mut payloads)=boundary_valid_wire(&previous);
    previous.validate_payloads(ProjectLimits::default(),&cancel).unwrap();
    let control=TransferReceiver::new_reusing(descriptor.clone(),ProjectLimits::default(),&previous).unwrap();
    assert!(control.missing_payloads().is_empty());
    let raw=|id|match descriptor.resources[&id] {VerifiedResource::Bytes{payload,..}=>payload,_=>panic!("byte resource")};
    let first=raw(boundary_id(190));let second=raw(boundary_id(191));assert_ne!(first,second);
    let merged=boundary_merge_byte_payloads(&mut descriptor,&mut payloads,first,second);
    for byte in [b'A',b'B'] {
        let mut supplied=payloads.clone();supplied[merged]=vec![byte;64];
        let receiver=TransferReceiver::new_reusing(descriptor.clone(),ProjectLimits::default(),&previous).unwrap();
        assert_eq!(receiver.missing_payloads(),vec![merged],"two distinct prior owners cannot supply one merged payload");
        let received=boundary_reuse_receive(&previous,descriptor.clone(),&supplied);
        assert!(!received.payload_trusted(merged));
        for id in [boundary_id(190),boundary_id(191)] {
            let VerifiedResource::Bytes{encoded:Some(encoded),..}=&descriptor.resources[&id] else{panic!("cache")};
            assert!(received.payload_trusted(encoded.payload),"unchanged caches should exercise reuse");
        }
        assert!(received.validate_payloads(ProjectLimits::default(),&cancel).is_err(),"fresh merged raw bytes skipped a reused cache relationship");
        assert!(received.adopt_verified(ProjectLimits::default(),&cancel).is_err(),"one of two conflicting cache roles reached publication");
    }
}

#[test]
fn transfer_boundary_reuse_existing_binding_with_new_private_role_requires_each_check() {
    let cancel=AtomicBool::new(false);let previous=boundary_cached_metadata_transfer();
    let (mut descriptor,payloads)=boundary_valid_wire(&previous);
    previous.validate_payloads(ProjectLimits::default(),&cancel).unwrap();
    let VerifiedResource::Bytes{payload:raw,encoded:Some(encoded)}=&descriptor.resources[&boundary_id(150)] else{panic!("cached metadata")};
    let raw=*raw;let cache=encoded.payload;let private=boundary_id(192);
    descriptor.private_resources.insert(private,(json!({"id":private,"type":"future.bytes/1","data":{},"encoding":"future.raw/1",
        "bytes":"64","crc32":format!("{:08x}",crc32fast::hash(&[b'B';64]))}),raw));
    for byte in [b'A',b'B'] {
        let mut supplied=payloads.clone();supplied[raw]=vec![byte;64];
        let receiver=TransferReceiver::new_reusing(descriptor.clone(),ProjectLimits::default(),&previous).unwrap();
        assert_eq!(receiver.missing_payloads(),vec![raw],"the new private role cannot inherit existing raw approval");
        let received=boundary_reuse_receive(&previous,descriptor.clone(),&supplied);
        assert!(!received.payload_trusted(raw));assert!(received.payload_trusted(cache));
        assert!(received.validate_payloads(ProjectLimits::default(),&cancel).is_err(),"new CRC role or old encoded-cache role was not checked");
        assert!(received.adopt_verified(ProjectLimits::default(),&cancel).is_err(),"a payload that fails one of its roles reached publication");
    }
}

#[test]
fn transfer_boundary_private_selection_alias_requires_comparable_owner_and_exact_content() {
    let cancel=AtomicBool::new(false);
    for cached in [false,true] {
        let prepared=boundary_selection_transfer(cached);
        let (descriptor,payloads)=boundary_valid_wire(&prepared);
        let id=descriptor.selections[0].chunks[0];
        assert!(matches!(descriptor.resources[&id],VerifiedResource::Coverage{payload:Some(_)} )==cached);
        for value in [4u32,3] {
            let mut changed=descriptor.clone();let mut supplied=payloads.clone();
            let other=SelectionPixels::new([9,1],[0,0,9,1],vec![0x11111111*value,value]).unwrap();
            let bytes=other.package_chunks().unwrap()[0].to_vec();
            let mut record=boundary_resource_record(&mut changed,id).clone();
            record["bytes"]=bytes.len().to_string().into();record["crc32"]=format!("{:08x}",crc32fast::hash(&bytes)).into();
            let payload=supplied.len();changed.lengths.push(bytes.len() as u64);supplied.push(bytes);
            changed.private_resources.insert(id,(record,payload));
            let received=boundary_receive(changed,supplied).unwrap();
            if cached && value==4 {
                received.validate_payloads(ProjectLimits::default(),&cancel).unwrap();
                received.adopt_verified_with_selection(ProjectLimits::default(),&cancel).unwrap();
            } else {
                assert!(received.validate_payloads(ProjectLimits::default(),&cancel).is_err(),"same-ID private alias with cached={cached}, value={value} acquired validation");
                assert!(received.adopt_verified_with_selection(ProjectLimits::default(),&cancel).is_err(),"noncomparable or conflicting private alias reached publication");
            }
        }
    }
}

fn boundary_filtered_deflate(descriptor:crate::color::PixelDescriptor,samples:&[u8])->Vec<u8> {
    use std::io::Write;
    let stride=descriptor.bytes_per_pixel().unwrap();let pixels=samples.len()/stride;
    let mut planes=Vec::with_capacity(samples.len());
    for byte in 0..stride {planes.extend(samples.chunks_exact(stride).map(|pixel|pixel[byte]));}
    assert_eq!(planes.len(),pixels*stride);
    for row in planes.chunks_mut(crate::raster::TILE_SIZE as usize) {
        for index in (1..row.len()).rev() {row[index]=row[index].wrapping_sub(row[index-1]);}
    }
    let mut encoder=flate2::write::ZlibEncoder::new(Vec::new(),flate2::Compression::new(6));
    encoder.write_all(&planes).unwrap();encoder.finish().unwrap()
}

fn boundary_retained_deflate_tile_transfer()->(PreparedTransfer,PortableId,Vec<u8>) {
    let mut original=capture();let art=Arc::make_mut(&mut original.artwork);
    let tile=art.paint.iter().next().unwrap().2.base.as_ref().unwrap().image.tiles.values().next().unwrap().clone();
    let id=tile.resource_id();let samples=tile.decode().unwrap();let bytes:Arc<[u8]>=boundary_filtered_deflate(tile.descriptor,&samples).into();
    let decoded=crate::raster::TileBlob::from_filtered_deflate(id,tile.descriptor,&bytes).unwrap();
    assert_eq!(decoded.decode().unwrap(),samples,"filtered-deflate control must contain the same exact samples");
    let opaque=Arc::new(OpaqueResource{id,kind:"capy.raster-tile/1".into(),data:resources::encode_descriptor(tile.descriptor).unwrap(),
        encoding:"capy.deflate-tile/1".into(),extra_fields:Default::default(),backing:ImmutableBacking::new(Arc::new(bytes.clone())).unwrap(),
        offset:0,length:bytes.len() as u64,crc32:crc32fast::hash(&bytes)});
    let extensions=Arc::make_mut(&mut art.extensions);extensions.resources.insert(id,opaque);
    let record=boundary_id(193);extensions.records.insert(record,json!({"id":record,"type":"future.tile-note/1","ancillary":true,"copy_safe":true,"data":{"payload":resources::reference(id)}}));
    let prepared=PreparedTransfer::capture(&original,&AtomicBool::new(false)).unwrap();
    (prepared,id,samples)
}

#[test]
fn transfer_boundary_deflate_tile_aliases_compare_samples_with_fresh_and_reused_opaque_owners() {
    let cancel=AtomicBool::new(false);let (previous,id,samples)=boundary_retained_deflate_tile_transfer();
    let (descriptor,payloads)=boundary_valid_wire(&previous);
    let VerifiedResource::Tile{payload:raw,..}=descriptor.resources[&id] else{panic!("canonical tile")};
    let retained=descriptor.retained_resources[&id];assert_ne!(raw,retained);
    assert!(matches!(&previous.payloads[retained],TransferPayload::Opaque(_)));
    let control=boundary_reuse_receive(&previous,descriptor.clone(),&payloads);
    control.adopt_verified(ProjectLimits::default(),&cancel).unwrap();
    let pixels=resources::parse_descriptor(&descriptor.manifest["resources"].as_array().unwrap().iter().find(|record|record["id"]==json!(id)).unwrap()["data"]).unwrap();
    let mut changed_samples=samples.clone();changed_samples[0]^=1;
    let changed_tile=crate::raster::TileBlob::encode(pixels,&changed_samples).unwrap();
    let changed_deflate=boundary_filtered_deflate(pixels,&changed_samples);
    assert_eq!(crate::raster::TileBlob::from_filtered_deflate(id,pixels,&changed_deflate).unwrap().decode().unwrap(),changed_samples);
    let mut fresh_descriptor=descriptor.clone();let mut fresh_payloads=payloads.clone();
    boundary_replace_payload(&mut fresh_descriptor,&mut fresh_payloads,id,retained,changed_deflate);
    boundary_resource_record(&mut fresh_descriptor,id)["crc32"]=format!("{:08x}",crc32fast::hash(&fresh_payloads[retained])).into();
    let fresh=boundary_receive(fresh_descriptor,fresh_payloads).unwrap();
    assert!(fresh.validate_payloads(ProjectLimits::default(),&cancel).is_err(),"different deflated samples acquired alias approval");
    assert!(fresh.adopt_verified(ProjectLimits::default(),&cancel).is_err());
    let mut changed_descriptor=descriptor;let mut changed_payloads=payloads;
    let encoded=changed_tile.compressed().unwrap();changed_descriptor.lengths[raw]=encoded.len() as u64;changed_payloads[raw]=encoded.to_vec();
    let VerifiedResource::Tile{fingerprint,..}=changed_descriptor.resources.get_mut(&id).unwrap() else{unreachable!()};
    *fingerprint=changed_tile.encoded_fingerprint();
    let receiver=TransferReceiver::new_reusing(changed_descriptor.clone(),ProjectLimits::default(),&previous).unwrap();
    assert_eq!(receiver.missing_payloads(),vec![raw],"only canonical tile bytes should change");
    let reused=boundary_reuse_receive(&previous,changed_descriptor,&changed_payloads);
    assert!(!reused.payload_trusted(raw));assert!(reused.payload_trusted(retained));
    assert!(matches!(&reused.payloads[retained],TransferPayload::Opaque(_)),"must exercise the typed opaque early-continue");
    assert!(reused.validate_payloads(ProjectLimits::default(),&cancel).is_err(),"reused typed opaque bytes skipped comparison with a fresh canonical tile");
    assert!(reused.adopt_verified(ProjectLimits::default(),&cancel).is_err());
}
