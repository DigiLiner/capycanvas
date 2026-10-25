use super::values::{DecodeError, DecodeResult};
use crate::authored::vector::*;
use crate::color::{RgbColor, RgbSpace};
use std::sync::Arc;

pub const KIND_PARENT: u64 = PARENT;
const KIND_RESERVED: u64 = 8;
const KIND_FLAGS: u32 = 4;
const TAG_FLAGS: u32 = 5;
const TAG_RESERVED: u64 = 16;
const CLASS_SHIFT: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueClass { Data, Items, Slots, Nested }
impl ValueClass {
    fn of(tag: u64) -> Self { [Self::Data, Self::Items, Self::Slots, Self::Nested][(tag >> CLASS_SHIFT & 3) as usize] }
    const fn bits(self) -> u64 { (self as u64) << CLASS_SHIFT }
}
pub const fn kind_word(number: u64) -> u64 { number << KIND_FLAGS }
pub const fn tag_word(number: u64, flags: u64, class: ValueClass) -> u64 { number << TAG_FLAGS | flags | class.bits() }

pub const PATH: u64 = kind_word(1);
pub const FILL: u64 = kind_word(2);
pub const STROKE: u64 = kind_word(3);
pub const COLOR: u64 = kind_word(4);

pub const GEOMETRY: u64 = tag_word(1, 0, ValueClass::Data);
pub const FILL_RULE: u64 = tag_word(2, 0, ValueClass::Data);
pub const STYLE: u64 = tag_word(3, 0, ValueClass::Nested);
pub const HIDDEN: u64 = tag_word(4, 0, ValueClass::Data);
pub const NAME: u64 = tag_word(5, IGNORABLE | COPY_SAFE, ValueClass::Data);
pub const LOCKED: u64 = tag_word(6, IGNORABLE | COPY_SAFE, ValueClass::Data);
pub const PAINT: u64 = tag_word(7, 0, ValueClass::Nested);
pub const WIDTH: u64 = tag_word(8, 0, ValueClass::Data);
pub const CAP: u64 = tag_word(9, 0, ValueClass::Data);
pub const JOIN: u64 = tag_word(10, 0, ValueClass::Data);
pub const MITER_LIMIT: u64 = tag_word(11, 0, ValueClass::Data);
pub const PROFILE: u64 = tag_word(12, 0, ValueClass::Data);
pub const COLOR_VALUE: u64 = tag_word(13, 0, ValueClass::Data);
pub const LIVE_SHAPE: u64 = tag_word(14, IGNORABLE, ValueClass::Data);

pub struct Number { pub word: u64, pub name: &'static str, pub on: &'static [u64], pub since: u32 }
pub const KINDS: &[Number] = &[
    Number { word: PATH, name: "path", on: &[], since: 1 },
    Number { word: FILL, name: "fill", on: &[STYLE], since: 1 },
    Number { word: STROKE, name: "stroke", on: &[STYLE], since: 1 },
    Number { word: COLOR, name: "color", on: &[PAINT], since: 1 },
];
pub const TAGS: &[Number] = &[
    Number { word: GEOMETRY, name: "geometry", on: &[PATH], since: 1 },
    Number { word: FILL_RULE, name: "fill_rule", on: &[PATH], since: 1 },
    Number { word: STYLE, name: "style", on: &[PATH], since: 1 },
    Number { word: HIDDEN, name: "hidden", on: &[PATH], since: 1 },
    Number { word: NAME, name: "name", on: &[PATH], since: 1 },
    Number { word: LOCKED, name: "locked", on: &[PATH], since: 1 },
    Number { word: PAINT, name: "paint", on: &[FILL, STROKE], since: 1 },
    Number { word: WIDTH, name: "width", on: &[STROKE], since: 1 },
    Number { word: CAP, name: "cap", on: &[STROKE], since: 1 },
    Number { word: JOIN, name: "join", on: &[STROKE], since: 1 },
    Number { word: MITER_LIMIT, name: "miter_limit", on: &[STROKE], since: 1 },
    Number { word: PROFILE, name: "width_profile", on: &[STROKE], since: 1 },
    Number { word: COLOR_VALUE, name: "color", on: &[COLOR], since: 1 },
    Number { word: LIVE_SHAPE, name: "live_shape", on: &[PATH], since: 1 },
];

const SPACES: [RgbSpace; 4] = [RgbSpace::Srgb, RgbSpace::DisplayP3, RgbSpace::AdobeRgb, RgbSpace::ProPhoto];
const CAPS: [Cap; 3] = [Cap::Butt, Cap::Round, Cap::Square];
const JOINS: [Join; 3] = [Join::Miter, Join::Round, Join::Bevel];
fn code<T: PartialEq>(table: &[T], value: &T) -> u8 { table.iter().position(|entry| entry == value).expect("Every value has a wire code") as u8 }
fn coded<T: Copy>(table: &[T], value: &[u8], name: &str) -> DecodeResult<T> {
    match value { [code] => table.get(usize::from(*code)).copied().ok_or_else(|| unsupported(name)), _ => Err(invalid(name)) }
}

pub const MAX_RECORDS: usize = 1 << 20;
pub const MAX_PATH_ANCHORS: usize = 1 << 20;
pub const MAX_PROPERTY_BYTES: usize = 16 << 20;
pub const MAX_CHUNK_BYTES: usize = 64 << 20;
pub const MAX_DEPTH: u32 = 64;
const ANCHOR_BYTES: usize = 3;
const KNOT_BYTES: usize = 6;
const UNITS: f64 = 256.;
const MAX_UNITS: i64 = 1 << 31;
const KNOT_START: u8 = 1;
const KNOT_END: u8 = 2;
const KNOT_EVEN: u8 = 4;

#[derive(Default)]
pub struct Reading { pub open: u32, pub slots: usize, pub highest: u64 }
impl Reading {
    pub fn slots(slots: usize) -> Self { Self { slots, ..Self::default() } }
    fn note(&mut self, id: u64) { self.highest = self.highest.max(id); }
}

fn invalid(message: &str) -> DecodeError { DecodeError::Invalid(format!("Invalid vector data: {message}")) }
fn unsupported(message: &str) -> DecodeError { DecodeError::Unsupported(format!("Unsupported vector data: {message}")) }

fn varuint(mut value:u64)->([u8;10],usize) {
    let mut bytes=[0;10];let mut length=0;
    while value>=0x80 {bytes[length]=value as u8|0x80;length+=1;value>>=7;}
    bytes[length]=value as u8;(bytes,length+1)
}
pub fn put_varuint(out:&mut Vec<u8>,value:u64) {let (bytes,length)=varuint(value);out.extend_from_slice(&bytes[..length]);}
struct Bytes<'a> { data: &'a [u8], at: usize }
impl<'a> Bytes<'a> {
    fn new(data: &'a [u8]) -> Self { Self { data, at: 0 } }
    fn done(&self) -> bool { self.at == self.data.len() }
    fn take(&mut self, count: usize) -> DecodeResult<&'a [u8]> {
        let end = self.at.checked_add(count).filter(|end| *end <= self.data.len()).ok_or_else(|| invalid("truncated value"))?;
        let bytes = &self.data[self.at..end];
        self.at = end;
        Ok(bytes)
    }
    fn u8(&mut self) -> DecodeResult<u8> { Ok(self.take(1)?[0]) }
    fn f32(&mut self) -> DecodeResult<f32> { Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap())) }
    fn finite_f32(&mut self) -> DecodeResult<f32> {
        let value = self.f32()?;
        if value.is_finite() { Ok(value) } else { Err(invalid("nonfinite number")) }
    }
    fn f64(&mut self) -> DecodeResult<f64> {
        let value = f64::from_le_bytes(self.take(8)?.try_into().unwrap());
        if value.is_finite() { Ok(value) } else { Err(invalid("nonfinite number")) }
    }
    fn varuint(&mut self) -> DecodeResult<u64> {
        let mut value = 0u64;
        for (index, &byte) in self.data[self.at..].iter().enumerate().take(10) {
            value |= u64::from(byte & 0x7f) << (7 * index);
            if byte < 0x80 {
                if byte == 0 && index > 0 { return Err(invalid("noncanonical varuint")); }
                if index == 9 && byte > 1 { return Err(invalid("varuint overflow")); }
                self.at += index + 1;
                return Ok(value);
            }
        }
        Err(invalid(if self.data.len() - self.at < 10 { "truncated value" } else { "varuint overflow" }))
    }
    fn zigzag(&mut self) -> DecodeResult<i64> {
        let value = self.varuint()?;
        let value = (value >> 1) as i64 ^ -((value & 1) as i64);
        if value.unsigned_abs() >= MAX_UNITS as u64 { return Err(unsupported("coordinate exceeds the admitted range")); }
        Ok(value)
    }
    fn length(&mut self, maximum: usize) -> DecodeResult<usize> {
        let length = self.varuint()?;
        if length > (self.data.len() - self.at) as u64 { return Err(invalid("length exceeds its container")); }
        if length > maximum as u64 { return Err(unsupported("value exceeds the admitted size")); }
        Ok(length as usize)
    }
}

struct Framed<'a> { kind: u64, id: u64, properties: &'a [u8] }
fn framed<'a>(bytes: &mut Bytes<'a>, reading: &mut Reading) -> DecodeResult<Option<Framed<'a>>> {
    let kind = bytes.varuint()?;
    if kind == 0 { return Ok(None); }
    let id = bytes.varuint()?;
    reading.note(id);
    let length = bytes.length(MAX_PROPERTY_BYTES * 4)?;
    Ok(Some(Framed { kind, id, properties: bytes.take(length)? }))
}
struct Fields<'a>(Bytes<'a>);
impl<'a> Iterator for Fields<'a> {
    type Item = (u64, &'a [u8]);
    fn next(&mut self) -> Option<Self::Item> {
        if self.0.done() { return None; }
        let tag = self.0.varuint().ok()?;
        let length = self.0.length(MAX_PROPERTY_BYTES).ok()?;
        Some((tag, self.0.take(length).ok()?))
    }
}
fn properties(data: &[u8]) -> DecodeResult<Fields<'_>> {
    let mut bytes = Bytes::new(data);
    let mut previous = None;
    while !bytes.done() {
        let tag = bytes.varuint()?;
        let length = bytes.length(MAX_PROPERTY_BYTES)?;
        bytes.take(length)?;
        if previous.is_some_and(|previous| previous >= tag >> TAG_FLAGS) { return Err(invalid("properties out of order")); }
        previous = Some(tag >> TAG_FLAGS);
    }
    Ok(Fields(Bytes::new(data)))
}
fn listed(value: &[u8], mut accept: impl FnMut(u64) -> DecodeResult<()>) -> DecodeResult<()> {
    let mut bytes = Bytes::new(value);
    while !bytes.done() { accept(bytes.varuint()?)?; }
    Ok(())
}
fn validate_value(tag: u64, value: &[u8], depth: u32, reading: &mut Reading) -> DecodeResult<()> {
    match ValueClass::of(tag) {
        ValueClass::Nested => validate_stream(value, depth + 1, reading),
        ValueClass::Items => listed(value, |id| if id == 0 { Err(invalid("item identity")) } else { reading.note(id); Ok(()) }),
        ValueClass::Slots => listed(value, |index| if index < reading.slots as u64 { Ok(()) } else { Err(invalid("slot index out of range")) }),
        ValueClass::Data => Ok(()),
    }
}
fn validate_frame(data: &[u8], depth: u32, reading: &mut Reading) -> DecodeResult<()> {
    if depth > MAX_DEPTH { return Err(unsupported("nesting exceeds the admitted depth")); }
    for (tag, value) in properties(data)? { validate_value(tag, value, depth, reading)?; }
    Ok(())
}
fn validate_stream(data: &[u8], depth: u32, reading: &mut Reading) -> DecodeResult<()> {
    let mut bytes = Bytes::new(data);
    let mut open = 0u32;
    while !bytes.done() {
        match framed(&mut bytes, reading)? {
            None => open = open.checked_sub(1).ok_or_else(|| invalid("unbalanced end marker"))?,
            Some(record) => {
                validate_frame(record.properties, depth, reading)?;
                if record.kind & KIND_PARENT != 0 { open += 1; if open > MAX_DEPTH { return Err(unsupported("nesting exceeds the admitted depth")); } }
            }
        }
    }
    if open == 0 { Ok(()) } else { Err(invalid("unbalanced end marker")) }
}
fn opaque(record: &Framed) -> OpaqueRecord { OpaqueRecord { kind: record.kind, id: record.id, properties: record.properties.into() } }
fn unknown_kind(record: &Framed, depth: u32, reading: &mut Reading) -> DecodeResult<OpaqueRecord> {
    if record.kind & KIND_RESERVED != 0 || record.kind & IGNORABLE == 0 { return Err(unsupported("required record kind")); }
    validate_frame(record.properties, depth, reading)?;
    Ok(opaque(record))
}
fn same_number(a: u64, b: u64, shift: u32) -> bool { a >> shift == b >> shift }
fn check_kind(word: u64) -> DecodeResult<bool> {
    match KINDS.iter().find(|n| same_number(n.word, word, KIND_FLAGS)) {
        Some(n) if n.word == word => Ok(true),
        Some(_) => Err(invalid("record kind with altered flags")),
        None => Ok(false),
    }
}
fn unknown_property(tag: u64, value: &[u8], owner: u64, depth: u32, reading: &mut Reading, unknown: &mut Vec<UnknownProperty>) -> DecodeResult<()> {
    if let Some(n) = TAGS.iter().find(|n| same_number(n.word, tag, TAG_FLAGS)) {
        if n.word != tag { return Err(invalid("property with altered flags")); }
        if !n.on.contains(&owner) { return Err(unsupported("property on another record kind")); }
    }
    if tag & TAG_RESERVED != 0 || tag & IGNORABLE == 0 { return Err(unsupported("required property")); }
    validate_value(tag, value, depth, reading)?;
    unknown.push(UnknownProperty { tag, value: value.into() });
    Ok(())
}
fn finish(unknown: Vec<UnknownProperty>) -> Unknown { (!unknown.is_empty()).then(|| unknown.into()) }
fn flag(value: &[u8]) -> Option<bool> { match value { [0] => Some(false), [1] => Some(true), _ => None } }
fn exact<T>(bytes: Bytes, value: T) -> DecodeResult<T> { if bytes.done() { Ok(value) } else { Err(invalid("value has trailing bytes")) } }

fn decode_geometry(value: &[u8]) -> DecodeResult<PathGeometry> {
    let mut bytes = Bytes::new(value);
    let count = bytes.varuint()?;
    if count == 0 || count > (value.len() / 2) as u64 { return Err(invalid("subpath count")); }
    let mut subpaths = Vec::with_capacity(count as usize);
    let mut total = 0usize;
    let mut previous = [0i64; 2];
    for _ in 0..count {
        let anchors = bytes.varuint()?;
        let flags = bytes.u8()?;
        if anchors == 0 { return Err(invalid("empty subpath")); }
        if anchors > ((value.len() - bytes.at) / ANCHOR_BYTES) as u64 { return Err(invalid("anchor count exceeds the value")); }
        total += anchors as usize;
        if total > MAX_PATH_ANCHORS { return Err(unsupported("path exceeds the admitted anchor count")); }
        if flags > 1 { return Err(unsupported("subpath flags")); }
        let mut list = Vec::with_capacity(anchors as usize);
        for _ in 0..anchors {
            let header = bytes.u8()?;
            let kind = AnchorKind(header >> 2);
            let point = [previous[0] + bytes.zigzag()?, previous[1] + bytes.zigzag()?];
            if point.iter().any(|v| v.unsigned_abs() >= MAX_UNITS as u64) { return Err(unsupported("coordinate exceeds the admitted range")); }
            let mut handle = |present: bool| -> DecodeResult<Option<[i64; 2]>> { if present { Ok(Some([bytes.zigzag()?, bytes.zigzag()?])) } else { Ok(None) } };
            let incoming = handle(header & 2 != 0)?;
            let outgoing = handle(header & 1 != 0)?.unwrap_or([0; 2]);
            let incoming = incoming.unwrap_or(if kind == AnchorKind::SYMMETRIC { [-outgoing[0], -outgoing[1]] } else { [0; 2] });
            list.push(Anchor::from_units([point[0], point[1], incoming[0], incoming[1], outgoing[0], outgoing[1]], kind).ok_or_else(|| unsupported("coordinate exceeds the admitted precision"))?);
            previous = point;
        }
        subpaths.push(Subpath::new(&list, flags == 1));
    }
    exact(bytes, ())?;
    PathGeometry::new(subpaths).map_err(invalid)
}
fn decode_profile(value: &[u8]) -> DecodeResult<WidthProfile> {
    let mut bytes = Bytes::new(value);
    let count = bytes.varuint()?;
    if count == 0 || count > (value.len() / KNOT_BYTES) as u64 { return Err(invalid("width knot count")); }
    let mut segment = 0u32;
    let knots = (0..count).map(|_| {
        segment = u32::try_from(bytes.varuint()?).ok().and_then(|delta| segment.checked_add(delta)).ok_or_else(|| unsupported("width knot segment"))?;
        let flags = bytes.u8()?;
        if flags & !(KNOT_START | KNOT_END | KNOT_EVEN) != 0 { return Err(unsupported("width knot flags")); }
        let t = match flags & (KNOT_START | KNOT_END) { KNOT_START => 0., KNOT_END => 1., 0 => bytes.finite_f32()?, _ => return Err(invalid("width knot flags")) };
        let left = bytes.finite_f32()?;
        let right = if flags & KNOT_EVEN != 0 { left } else { bytes.finite_f32()? };
        Ok(WidthKnot::new(segment, f64::from(t), f64::from(left), f64::from(right)))
    }).collect::<DecodeResult<Vec<_>>>()?;
    exact(bytes, ())?;
    WidthProfile::new(knots).map_err(invalid)
}
fn decode_color(value: &[u8]) -> DecodeResult<RgbColor> {
    let mut bytes = Bytes::new(value);
    let (space, form) = (bytes.u8()?, bytes.u8()?);
    let rgba = [bytes.f32()?, bytes.f32()?, bytes.f32()?, bytes.f32()?];
    exact(bytes, ())?;
    let space = coded(&SPACES, &[space], "color space")?;
    match form {
        0 => RgbColor::new(space, rgba).map_err(|e| invalid(&e)),
        1 => RgbColor::from_linear(space, rgba).map_err(|e| invalid(&e)),
        _ => Err(unsupported("color form")),
    }
}
fn decode_paint(value: &[u8], depth: u32, reading: &mut Reading) -> DecodeResult<Paint> {
    let mut bytes = Bytes::new(value);
    let record = framed(&mut bytes, reading)?.ok_or_else(|| invalid("paint requires a record"))?;
    exact(bytes, ())?;
    if !check_kind(record.kind)? { return Ok(Paint::Opaque(unknown_kind(&record, depth, reading)?)); }
    if record.kind & KIND_PARENT != 0 { return Err(invalid("paint record has children")); }
    if record.kind != COLOR { return Err(unsupported("record kind in a paint")); }
    let (mut color, mut unknown) = (None, Vec::new());
    for (tag, value) in properties(record.properties)? {
        if tag == COLOR_VALUE { color = Some(decode_color(value)?); } else { unknown_property(tag, value, COLOR, depth, reading, &mut unknown)?; }
    }
    Ok(Paint::Color { color: color.ok_or_else(|| invalid("color paint requires a color"))?, unknown: finish(unknown) })
}
fn decode_entry(record: &Framed, depth: u32, reading: &mut Reading) -> DecodeResult<PaintEntry> {
    if !check_kind(record.kind)? { return Ok(PaintEntry::Opaque(unknown_kind(record, depth, reading)?)); }
    if record.kind != FILL && record.kind != STROKE { return Err(unsupported("record kind in a style")); }
    let mut paint = None;
    let mut stroke = Stroke::new(Paint::color(RgbColor::BLACK), Stroke::DEFAULT_WIDTH);
    let mut unknown = Vec::new();
    for (tag, value) in properties(record.properties)? {
        match tag {
            PAINT => paint = Some(decode_paint(value, depth + 1, reading)?),
            WIDTH if record.kind == STROKE => stroke.width = width(value)?,
            CAP if record.kind == STROKE => stroke.cap = coded(&CAPS, value, "stroke cap")?,
            JOIN if record.kind == STROKE => stroke.join = coded(&JOINS, value, "stroke join")?,
            MITER_LIMIT if record.kind == STROKE => stroke.miter_limit = exact_f64(value)?,
            PROFILE if record.kind == STROKE => stroke.profile = Some(Arc::new(decode_profile(value)?)),
            _ => unknown_property(tag, value, record.kind, depth, reading, &mut unknown)?,
        }
    }
    let paint = paint.ok_or_else(|| invalid("paint entry requires a paint"))?;
    if record.kind == FILL { return Ok(PaintEntry::Fill(Fill { paint, unknown: finish(unknown) })); }
    stroke.paint = paint;
    stroke.unknown = finish(unknown);
    stroke.validate().map_err(invalid)?;
    Ok(PaintEntry::Stroke(stroke))
}
fn exact_f64(value: &[u8]) -> DecodeResult<f64> { let mut bytes = Bytes::new(value); let v = bytes.f64()?; exact(bytes, v) }
fn width(value: &[u8]) -> DecodeResult<f64> {
    let mut bytes = Bytes::new(value);
    let units = bytes.varuint()?;
    if units >= MAX_UNITS as u64 { return Err(unsupported("stroke width exceeds the admitted range")); }
    exact(bytes, units as f64 / UNITS)
}
fn decode_style(value: &[u8], depth: u32, reading: &mut Reading) -> DecodeResult<Style> {
    let mut bytes = Bytes::new(value);
    let mut entries = Vec::new();
    let mut open = 0u32;
    while !bytes.done() {
        match framed(&mut bytes, reading)? {
            None => {
                open = open.checked_sub(1).ok_or_else(|| invalid("unbalanced end marker"))?;
                entries.push(PaintEntry::Opaque(OpaqueRecord { kind: 0, id: 0, properties: Arc::from([]) }));
            }
            Some(record) if open > 0 => {
                validate_frame(record.properties, depth + 1, reading)?;
                if record.kind & KIND_PARENT != 0 { open += 1; }
                entries.push(PaintEntry::Opaque(opaque(&record)));
            }
            Some(record) => {
                let entry = decode_entry(&record, depth + 1, reading)?;
                if record.kind & KIND_PARENT != 0 { open += 1; }
                entries.push(entry);
            }
        }
        if open > MAX_DEPTH { return Err(unsupported("nesting exceeds the admitted depth")); }
    }
    if open != 0 { return Err(invalid("unbalanced end marker")); }
    Ok(Style::new(entries))
}
fn decode_path(record: &Framed, reading: &mut Reading) -> DecodeResult<PathItem> {
    if record.id == 0 { return Err(invalid("path identity")); }
    let (mut geometry, mut style) = (None, None);
    let (mut fill_rule, mut hidden, mut locked, mut name) = (FillRule::NonZero, false, false, None);
    let mut unknown = Vec::new();
    let mut shape = None;
    for (tag, value) in properties(record.properties)? {
        match tag {
            GEOMETRY => geometry = Some(decode_geometry(value)?),
            FILL_RULE => fill_rule = match value { [0] => FillRule::NonZero, [1] => FillRule::EvenOdd, [_] => return Err(unsupported("fill rule")), _ => return Err(invalid("fill rule")) },
            STYLE => style = Some(Arc::new(decode_style(value, 1, reading)?)),
            HIDDEN => hidden = flag(value).ok_or_else(|| invalid("hidden flag"))?,
            NAME => match std::str::from_utf8(value) {
                Ok(text) if text.len() <= crate::authored::MAX_NAME_BYTES => name = Some(text.into()),
                _ => unknown.push(UnknownProperty { tag, value: value.into() }),
            },
            LOCKED => match flag(value) { Some(value) => locked = value, None => unknown.push(UnknownProperty { tag, value: value.into() }) },
            LIVE_SHAPE => match decode_shape(value) { Some(recipe) => shape = Some(recipe), None => unknown.push(UnknownProperty { tag, value: value.into() }) },
            _ => unknown_property(tag, value, PATH, 1, reading, &mut unknown)?,
        }
    }
    let mut path = PathItem::new(ItemId(record.id), Arc::new(geometry.ok_or_else(|| invalid("path requires geometry"))?), style.unwrap_or_default());
    (path.fill_rule, path.hidden, path.locked, path.name) = (fill_rule, hidden, locked, name);
    if let Some((shape,print))=shape && print==fingerprint(&path)? {path.shape=Some(Arc::new(shape));}
    path.unknown = finish(unknown);
    path.validate().map_err(invalid)?;
    Ok(path)
}

pub fn decode_chunk(data: &[u8], expected: usize, reading: &mut Reading) -> DecodeResult<VectorChunk> {
    if expected == 0 || expected > CHUNK_RECORDS { return Err(invalid("chunk record count")); }
    let mut bytes = Bytes::new(data);
    let mut records = Vec::with_capacity(expected);
    while !bytes.done() {
        if records.len() == expected { return Err(invalid("chunk holds more records than declared")); }
        let record = match framed(&mut bytes, reading)? {
            None => { reading.open = reading.open.checked_sub(1).ok_or_else(|| invalid("unbalanced end marker"))?; Record::End }
            Some(record) if reading.open > 0 => { validate_frame(record.properties, reading.open, reading)?; if record.kind & KIND_PARENT != 0 { reading.open += 1; } Record::Opaque(opaque(&record)) }
            Some(record) => {
                let decoded = if check_kind(record.kind)? {
                    if record.kind != PATH { return Err(unsupported("record kind in an item list")); }
                    Record::Path(decode_path(&record, reading)?)
                } else { Record::Opaque(unknown_kind(&record, 0, reading)?) };
                if record.kind & KIND_PARENT != 0 { reading.open += 1; }
                decoded
            }
        };
        if reading.open > MAX_DEPTH { return Err(unsupported("nesting exceeds the admitted depth")); }
        records.push(record);
    }
    if records.len() != expected { return Err(invalid("chunk holds fewer records than declared")); }
    let chunk = VectorChunk::new(records);
    chunk.note_encoded_len(data.len());
    Ok(chunk)
}
pub fn decode_object(chunks: &[(&[u8], usize)], reading: &mut Reading) -> DecodeResult<Vec<VectorChunk>> {
    let mut total = 0usize;
    let mut ids = std::collections::HashSet::new();
    let decoded = chunks.iter().map(|(data, count)| {
        total += count;
        if total > MAX_RECORDS { return Err(unsupported("vector object exceeds the admitted record count")); }
        let chunk = decode_chunk(data, *count, reading)?;
        for id in chunk.records().iter().filter_map(Record::id) {
            if !ids.insert(id) { return Err(invalid("duplicate item identity")); }
        }
        Ok(chunk)
    }).collect::<DecodeResult<Vec<_>>>()?;
    if reading.open != 0 { return Err(invalid("unbalanced end marker")); }
    Ok(decoded)
}

trait Sink {fn write(&mut self,bytes:&[u8])->DecodeResult<()>;}
struct Output {bytes:crate::memory::DecodedBytes,limit:usize}
impl Output {
    fn new(limit:usize)->DecodeResult<Self> {Ok(Self {bytes:crate::memory::DecodedBytes::zeroed(0)?,limit})}
    fn push(&mut self,byte:u8)->DecodeResult<()> {self.write(&[byte])}
    fn finish(self)->crate::memory::DecodedBytes {self.bytes}
}
impl Sink for Output {
    fn write(&mut self,bytes:&[u8])->DecodeResult<()> {
        if self.bytes.len().checked_add(bytes.len()).is_none_or(|length|length>self.limit) {return Err(unsupported("encoding exceeds the admitted byte count"));}
        self.bytes.extend_from_slice(bytes)?;Ok(())
    }
}
impl Sink for sha2::Sha256 {
    fn write(&mut self,bytes:&[u8])->DecodeResult<()> {sha2::Digest::update(self,bytes);Ok(())}
}
fn write_varuint(out:&mut impl Sink,value:u64)->DecodeResult<()> {let (bytes,length)=varuint(value);out.write(&bytes[..length])}
fn put_property(out:&mut impl Sink,tag:u64,value:&[u8])->DecodeResult<()> {
    write_varuint(out,tag)?;write_varuint(out,value.len() as u64)?;out.write(value)
}
fn put_record(out:&mut impl Sink,kind:u64,id:u64,properties:&[u8])->DecodeResult<()> {
    if properties.len()>MAX_PROPERTY_BYTES {return Err(unsupported("record properties exceed the admitted byte count"));}
    write_varuint(out,kind)?;
    if kind!=0 {write_varuint(out,id)?;write_varuint(out,properties.len() as u64)?;out.write(properties)?;}
    Ok(())
}
pub fn put_zigzag(out:&mut Vec<u8>,value:i64) {put_varuint(out,((value<<1)^(value>>63)) as u64);}
fn write_zigzag(out:&mut impl Sink,value:i64)->DecodeResult<()> {write_varuint(out,((value<<1)^(value>>63)) as u64)}
enum Property<'a> {Owned(crate::memory::DecodedBytes),Borrowed(&'a [u8])}
impl Property<'_> {fn bytes(&self)->&[u8] {match self {Self::Owned(bytes)=>bytes,Self::Borrowed(bytes)=>bytes}}}
struct Properties<'a> {list:Vec<(u64,usize,Property<'a>)>,memory:crate::memory::Reservation<'static>,length:usize}
impl<'a> Properties<'a> {
    fn new()->DecodeResult<Self> {Ok(Self {list:Vec::new(),memory:crate::memory::images().reserve(0)?,length:0})}
    fn insert(&mut self,tag:u64,value:Property<'a>)->DecodeResult<()> {
        let length=value.bytes().len();
        let length=self.length.checked_add(varuint(tag).1).and_then(|n|n.checked_add(varuint(length as u64).1)).and_then(|n|n.checked_add(length))
            .filter(|length|*length<=MAX_PROPERTY_BYTES).ok_or_else(||unsupported("properties exceed the admitted byte count"))?;
        if self.list.len()==self.list.capacity() {
            let capacity=self.list.capacity().saturating_mul(2).clamp(4,MAX_PROPERTY_BYTES/2);
            let bytes=capacity.checked_mul(std::mem::size_of::<(u64,usize,Property<'a>)>()).ok_or(crate::memory::MemoryError)?;
            self.memory.resize(self.list.capacity().checked_mul(std::mem::size_of::<(u64,usize,Property<'a>)>()).and_then(|old|old.checked_add(bytes)).ok_or(crate::memory::MemoryError)?)?;
            self.list.try_reserve_exact(capacity-self.list.len()).map_err(|_|crate::memory::MemoryError)?;
            self.memory.resize(self.list.capacity().checked_mul(std::mem::size_of::<(u64,usize,Property<'a>)>()).ok_or(crate::memory::MemoryError)?)?;
        }
        self.list.push((tag,self.list.len(),value));self.length=length;Ok(())
    }
    fn put(&mut self,tag:u64,value:crate::memory::DecodedBytes)->DecodeResult<()> {self.insert(tag,Property::Owned(value))}
    fn fixed(&mut self,tag:u64,value:&[u8])->DecodeResult<()> {self.put(tag,crate::memory::DecodedBytes::copy_from(value)?)}
    fn borrowed(&mut self,tag:u64,value:&'a [u8])->DecodeResult<()> {self.insert(tag,Property::Borrowed(value))}
    fn unknown(&mut self,unknown:&'a Unknown)->DecodeResult<()> {for p in unknown.iter().flat_map(|u|u.iter()) {self.borrowed(p.tag,&p.value)?;}Ok(())}
    fn write(&mut self,out:&mut impl Sink)->DecodeResult<()> {
        self.list.sort_unstable_by_key(|(tag,position,_)|(*tag>>TAG_FLAGS,*position));
        for (tag,_,value) in &self.list {put_property(out,*tag,value.bytes())?;}Ok(())
    }
    fn bytes(&mut self)->DecodeResult<crate::memory::DecodedBytes> {let mut out=Output::new(MAX_PROPERTY_BYTES)?;self.write(&mut out)?;Ok(out.finish())}
    fn fingerprint(&mut self)->DecodeResult<[u8;16]> {
        use sha2::Digest;
        let mut hash=sha2::Sha256::new();write_varuint(&mut hash,PATH)?;self.write(&mut hash)?;
        Ok(hash.finalize()[..16].try_into().unwrap())
    }
}
pub fn encode_geometry(geometry:&PathGeometry)->DecodeResult<crate::memory::DecodedBytes> {
    if geometry.anchor_count()>MAX_PATH_ANCHORS {return Err(unsupported("too many path anchors"));}
    let mut out=Output::new(MAX_PROPERTY_BYTES)?;
    write_varuint(&mut out,geometry.subpaths().len() as u64)?;
    let mut previous=[0i64;2];
    for subpath in geometry.subpaths() {
        write_varuint(&mut out,subpath.len() as u64)?;out.push(u8::from(subpath.closed))?;
        for a in subpath.anchors() {
            let [px,py,ix,iy,ox,oy]=a.units();
            let (point,incoming,outgoing)=([px,py],[ix,iy],[ox,oy]);
            let implied=if a.kind==AnchorKind::SYMMETRIC {[-outgoing[0],-outgoing[1]]}else{[0;2]};
            let (has_in,has_out)=(incoming!=implied,outgoing!=[0;2]);
            out.push(a.kind.0<<2|u8::from(has_in)<<1|u8::from(has_out))?;
            for axis in 0..2 {write_zigzag(&mut out,point[axis]-previous[axis])?;}
            for handle in [incoming].into_iter().filter(|_|has_in).chain([outgoing].into_iter().filter(|_|has_out)) {
                for value in handle {write_zigzag(&mut out,value)?;}
            }
            previous=point;
        }
    }
    Ok(out.finish())
}
fn encode_profile(profile:&WidthProfile)->DecodeResult<crate::memory::DecodedBytes> {
    let mut out=Output::new(MAX_PROPERTY_BYTES)?;write_varuint(&mut out,profile.knots().len() as u64)?;
    let mut segment=0;
    for k in profile.knots() {
        write_varuint(&mut out,u64::from(k.segment()-segment))?;segment=k.segment();
        let [t,left,right]=k.bits();
        let place=if t.to_bits()==0f32.to_bits() {KNOT_START}else if t.to_bits()==1f32.to_bits() {KNOT_END}else{0};
        let even=left.to_bits()==right.to_bits();out.push(place|if even {KNOT_EVEN}else{0})?;
        if place==0 {out.write(&t.to_le_bytes())?;}
        out.write(&left.to_le_bytes())?;if !even {out.write(&right.to_le_bytes())?;}
    }
    Ok(out.finish())
}
fn encode_color(color:&RgbColor)->DecodeResult<crate::memory::DecodedBytes> {
    let space=code(&SPACES,&color.space);
    let (form,rgba)=match color.linear_rgb {Some([r,g,b])=>(1,[r,g,b,color.rgba[3]]),None=>(0,color.rgba)};
    let mut out=Output::new(MAX_PROPERTY_BYTES)?;out.write(&[space,form])?;
    for v in rgba {out.write(&v.to_le_bytes())?;}Ok(out.finish())
}
fn encode_opaque(out:&mut impl Sink,record:&OpaqueRecord)->DecodeResult<()> {put_record(out,record.kind,record.id,&record.properties)}
fn encode_paint(paint:&Paint)->DecodeResult<crate::memory::DecodedBytes> {
    let mut out=Output::new(MAX_PROPERTY_BYTES)?;
    match paint {
        Paint::Color {color,unknown}=>{let mut p=Properties::new()?;p.put(COLOR_VALUE,encode_color(color)?)?;p.unknown(unknown)?;put_record(&mut out,COLOR,0,&p.bytes()?)?;},
        Paint::Opaque(record)=>encode_opaque(&mut out,record)?,
    }
    Ok(out.finish())
}
pub(super) fn encode_style(style:&Style)->DecodeResult<crate::memory::DecodedBytes> {
    let mut out=Output::new(MAX_PROPERTY_BYTES)?;
    for entry in style.entries.iter() {
        match entry {
            PaintEntry::Fill(fill)=>{let mut p=Properties::new()?;p.put(PAINT,encode_paint(&fill.paint)?)?;p.unknown(&fill.unknown)?;put_record(&mut out,FILL,0,&p.bytes()?)?;},
            PaintEntry::Stroke(s)=>{
                let mut p=Properties::new()?;p.put(PAINT,encode_paint(&s.paint)?)?;
                if s.width!=Stroke::DEFAULT_WIDTH {let mut width=Output::new(MAX_PROPERTY_BYTES)?;write_varuint(&mut width,(s.width*UNITS).round() as u64)?;p.put(WIDTH,width.finish())?;}
                if s.cap!=Cap::Butt {p.fixed(CAP,&[code(&CAPS,&s.cap)])?;}
                if s.join!=Join::Miter {p.fixed(JOIN,&[code(&JOINS,&s.join)])?;}
                if s.miter_limit!=Stroke::DEFAULT_MITER_LIMIT {p.fixed(MITER_LIMIT,&s.miter_limit.to_le_bytes())?;}
                if let Some(profile)=&s.profile {p.put(PROFILE,encode_profile(profile)?)?;}
                p.unknown(&s.unknown)?;put_record(&mut out,STROKE,0,&p.bytes()?)?;
            },
            PaintEntry::Opaque(record)=>encode_opaque(&mut out,record)?,
        }
    }
    Ok(out.finish())
}
fn required(path:&PathItem)->DecodeResult<Properties<'_>> {
    let mut p=Properties::new()?;p.put(GEOMETRY,encode_geometry(&path.geometry)?)?;
    if path.fill_rule!=FillRule::NonZero {p.fixed(FILL_RULE,&[1])?;}
    if !path.style.entries.is_empty() {p.put(STYLE,encode_style(&path.style)?)?;}
    if path.hidden {p.fixed(HIDDEN,&[1])?;}Ok(p)
}
pub(super) fn fingerprint(path:&PathItem)->DecodeResult<[u8;16]> {required(path)?.fingerprint()}
pub(super) fn encode_shape(shape:&LiveShape,print:[u8;16])->DecodeResult<crate::memory::DecodedBytes> {
    let (kind,extra,length)=match shape.kind {
        ShapeKind::Rectangle {radii}=>(1,radii,4),
        ShapeKind::Ellipse=>(2,[0.;4],0),
        ShapeKind::Polygon {corners}=>(3,[f64::from(corners),0.,0.,0.],1),
        ShapeKind::Star {points,inner}=>(4,[f64::from(points),inner,0.,0.],2),
    };
    let mut out=Output::new(MAX_PROPERTY_BYTES)?;out.push(kind)?;
    for v in [shape.center[0],shape.center[1],shape.size[0],shape.size[1],shape.rotation].into_iter().chain(extra[..length].iter().copied()) {out.write(&v.to_le_bytes())?;}
    out.write(&print)?;Ok(out.finish())
}
fn decode_shape(value: &[u8]) -> Option<(LiveShape, [u8; 16])> {
    let (&kind, rest) = value.split_first()?;
    let numbers = match kind { 1 => 9, 2 => 5, 3 => 6, 4 => 7, _ => return None };
    if rest.len() != numbers * 8 + 16 { return None; }
    let v: Vec<f64> = rest[..numbers * 8].as_chunks::<8>().0.iter().map(|b| f64::from_le_bytes(*b)).collect();
    if !v.iter().all(|n| n.is_finite()) || v[2] < 0. || v[3] < 0. { return None; }
    let count = |n: f64| (n.fract() == 0. && n >= 0. && n <= f64::from(u32::MAX)).then_some(n as u32);
    let kind = match kind {
        1 => ShapeKind::Rectangle { radii: [v[5], v[6], v[7], v[8]] },
        2 => ShapeKind::Ellipse,
        3 => ShapeKind::Polygon { corners: count(v[5])? },
        _ => ShapeKind::Star { points: count(v[5])?, inner: v[6] },
    };
    if !kind.valid() { return None; }
    Some((LiveShape { kind, center: [v[0], v[1]], size: [v[2], v[3]], rotation: v[4] }, rest[numbers * 8..].try_into().unwrap()))
}
fn encode_path(out:&mut impl Sink,path:&PathItem)->DecodeResult<()> {
    let mut p=required(path)?;
    if let Some(shape)=&path.shape {let print=p.fingerprint()?;p.put(LIVE_SHAPE,encode_shape(shape,print)?)?;}
    if let Some(name)=path.name.as_deref().filter(|n|!n.is_empty()) {p.borrowed(NAME,name.as_bytes())?;}
    if path.locked {p.fixed(LOCKED,&[1])?;}p.unknown(&path.unknown)?;
    put_record(out,PATH,path.id.0,&p.bytes()?)
}
pub fn encode_chunk(chunk:&VectorChunk)->DecodeResult<crate::memory::DecodedBytes> {
    if chunk.records().len()>MAX_RECORDS {return Err(unsupported("too many records"));}
    let mut out=Output::new(MAX_CHUNK_BYTES)?;
    for record in chunk.records() {
        match record {
            Record::Path(path)=>encode_path(&mut out,path)?,
            Record::Opaque(record)=>encode_opaque(&mut out,record)?,
            Record::End=>out.push(0)?,
        }
    }
    let out=out.finish();chunk.note_encoded_len(out.len());Ok(out)
}

#[cfg(test)]
mod encoding_tests {
    use super::*;

    #[test]
    fn output_refuses_a_whole_write_before_growth_and_remains_usable() {
        let mut out=Output::new(4).unwrap();out.write(&[1,2,3]).unwrap();
        let capacity=out.bytes.capacity();
        assert!(matches!(out.write(&[4,5]),Err(DecodeError::Unsupported(_))));
        assert_eq!(out.bytes.as_slice(),[1,2,3]);assert_eq!(out.bytes.capacity(),capacity);
        out.push(4).unwrap();assert_eq!(out.finish().as_slice(),[1,2,3,4]);
    }
    #[test]
    fn first_encoding_rejects_oversized_properties_before_copying_or_caching() {
        let bytes:Arc<[u8]>=vec![0;MAX_PROPERTY_BYTES+1].into();
        let mut properties=Properties::new().unwrap();
        assert!(matches!(properties.borrowed(GEOMETRY,&bytes),Err(DecodeError::Unsupported(_))));
        assert!(properties.list.is_empty());assert_eq!(properties.list.capacity(),0);assert_eq!(properties.length,0);
        let chunk=VectorChunk::new(vec![Record::Opaque(OpaqueRecord {kind:kind_word(99)|IGNORABLE|COPY_SAFE,id:1,properties:bytes})]);
        assert!(chunk.encoded_len().is_none());
        assert!(matches!(encode_chunk(&chunk),Err(DecodeError::Unsupported(_))));
        assert!(chunk.encoded_len().is_none());
    }
}
