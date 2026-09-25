//! Minimal fragmented MP4 (ISO BMFF) writer for live MSE playback.
//!
//! Produces an init segment (`ftyp`+`moov`) and media fragments (`moof`+`mdat`)
//! suitable for `MediaSource` / `SourceBuffer` append.

use std::io::Write;

fn be32(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

fn be64(v: u64) -> [u8; 8] {
    v.to_be_bytes()
}

fn write_box<F: FnOnce(&mut Vec<u8>)>(out: &mut Vec<u8>, typ: &[u8; 4], body: F) {
    let start = out.len();
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(typ);
    body(out);
    let size = (out.len() - start) as u32;
    out[start..start + 4].copy_from_slice(&be32(size));
}

fn write_fullbox_header(out: &mut Vec<u8>, version: u8, flags: u32) {
    out.push(version);
    out.extend_from_slice(&[(flags >> 16) as u8, (flags >> 8) as u8, flags as u8]);
}

/// Build an init segment from a retina `VideoSampleEntry` box and dimensions.
pub fn build_init_segment(sample_entry: &[u8], width: u16, height: u16, timescale: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(1024);

    write_box(&mut out, b"ftyp", |b| {
        b.extend_from_slice(b"isom");
        b.extend_from_slice(&be32(0x200));
        b.extend_from_slice(b"isom");
        b.extend_from_slice(b"iso6");
        b.extend_from_slice(b"avc1");
        b.extend_from_slice(b"mp41");
    });

    write_box(&mut out, b"moov", |moov| {
        write_box(moov, b"mvhd", |b| {
            write_fullbox_header(b, 0, 0);
            b.extend_from_slice(&be32(0)); // creation_time
            b.extend_from_slice(&be32(0)); // modification_time
            b.extend_from_slice(&be32(timescale));
            b.extend_from_slice(&be32(0)); // duration
            b.extend_from_slice(&be32(0x00010000)); // rate 1.0
            b.extend_from_slice(&0x0100u16.to_be_bytes()); // volume
            b.extend_from_slice(&[0; 10]); // reserved
            // unity matrix
            b.extend_from_slice(&be32(0x00010000));
            b.extend_from_slice(&be32(0));
            b.extend_from_slice(&be32(0));
            b.extend_from_slice(&be32(0));
            b.extend_from_slice(&be32(0x00010000));
            b.extend_from_slice(&be32(0));
            b.extend_from_slice(&be32(0));
            b.extend_from_slice(&be32(0));
            b.extend_from_slice(&be32(0x40000000));
            b.extend_from_slice(&[0; 24]);
            b.extend_from_slice(&be32(2)); // next_track_ID
        });

        write_box(moov, b"trak", |trak| {
            write_box(trak, b"tkhd", |b| {
                write_fullbox_header(b, 0, 0x7); // track enabled|in_movie|in_preview
                b.extend_from_slice(&be32(0));
                b.extend_from_slice(&be32(0));
                b.extend_from_slice(&be32(1)); // track_ID
                b.extend_from_slice(&be32(0)); // reserved
                b.extend_from_slice(&be32(0)); // duration
                b.extend_from_slice(&[0; 8]);
                b.extend_from_slice(&0u16.to_be_bytes()); // layer
                b.extend_from_slice(&0u16.to_be_bytes()); // alternate_group
                b.extend_from_slice(&0u16.to_be_bytes()); // volume
                b.extend_from_slice(&0u16.to_be_bytes());
                b.extend_from_slice(&be32(0x00010000));
                b.extend_from_slice(&be32(0));
                b.extend_from_slice(&be32(0));
                b.extend_from_slice(&be32(0));
                b.extend_from_slice(&be32(0x00010000));
                b.extend_from_slice(&be32(0));
                b.extend_from_slice(&be32(0));
                b.extend_from_slice(&be32(0));
                b.extend_from_slice(&be32(0x40000000));
                b.extend_from_slice(&((width as u32) << 16).to_be_bytes());
                b.extend_from_slice(&((height as u32) << 16).to_be_bytes());
            });

            write_box(trak, b"mdia", |mdia| {
                write_box(mdia, b"mdhd", |b| {
                    write_fullbox_header(b, 0, 0);
                    b.extend_from_slice(&be32(0));
                    b.extend_from_slice(&be32(0));
                    b.extend_from_slice(&be32(timescale));
                    b.extend_from_slice(&be32(0));
                    b.extend_from_slice(&0u16.to_be_bytes()); // language
                    b.extend_from_slice(&0u16.to_be_bytes());
                });

                write_box(mdia, b"hdlr", |b| {
                    write_fullbox_header(b, 0, 0);
                    b.extend_from_slice(&be32(0));
                    b.extend_from_slice(b"vide");
                    b.extend_from_slice(&[0; 12]);
                    b.extend_from_slice(b"VideoHandler\0");
                });

                write_box(mdia, b"minf", |minf| {
                    write_box(minf, b"vmhd", |b| {
                        write_fullbox_header(b, 0, 1);
                        b.extend_from_slice(&[0; 8]);
                    });
                    write_box(minf, b"dinf", |dinf| {
                        write_box(dinf, b"dref", |b| {
                            write_fullbox_header(b, 0, 0);
                            b.extend_from_slice(&be32(1));
                            write_box(b, b"url ", |u| {
                                write_fullbox_header(u, 0, 1); // self-contained
                            });
                        });
                    });
                    write_box(minf, b"stbl", |stbl| {
                        write_box(stbl, b"stsd", |b| {
                            write_fullbox_header(b, 0, 0);
                            b.extend_from_slice(&be32(1));
                            b.extend_from_slice(sample_entry);
                        });
                        write_box(stbl, b"stts", |b| {
                            write_fullbox_header(b, 0, 0);
                            b.extend_from_slice(&be32(0));
                        });
                        write_box(stbl, b"stsc", |b| {
                            write_fullbox_header(b, 0, 0);
                            b.extend_from_slice(&be32(0));
                        });
                        write_box(stbl, b"stsz", |b| {
                            write_fullbox_header(b, 0, 0);
                            b.extend_from_slice(&be32(0));
                            b.extend_from_slice(&be32(0));
                        });
                        write_box(stbl, b"stco", |b| {
                            write_fullbox_header(b, 0, 0);
                            b.extend_from_slice(&be32(0));
                        });
                    });
                });
            });
        });

        write_box(moov, b"mvex", |mvex| {
            write_box(mvex, b"trex", |b| {
                write_fullbox_header(b, 0, 0);
                b.extend_from_slice(&be32(1)); // track_ID
                b.extend_from_slice(&be32(1)); // default_sample_description_index
                b.extend_from_slice(&be32(0)); // default_sample_duration
                b.extend_from_slice(&be32(0)); // default_sample_size
                b.extend_from_slice(&be32(0)); // default_sample_flags
            });
        });
    });

    out
}

/// One sample inside a media fragment.
pub struct Sample<'a> {
    pub data: &'a [u8],
    pub duration: u32,
    pub is_keyframe: bool,
}

/// Build a media fragment (`moof`+`mdat`) for one or more samples.
pub fn build_fragment(sequence: u32, base_decode_time: u64, samples: &[Sample<'_>]) -> Vec<u8> {
    let mut mdat_payload = Vec::new();
    for s in samples {
        mdat_payload.extend_from_slice(s.data);
    }

    // First pass: build moof with placeholder data_offset, then patch.
    let mut moof = Vec::new();
    write_box(&mut moof, b"moof", |moof| {
        write_box(moof, b"mfhd", |b| {
            write_fullbox_header(b, 0, 0);
            b.extend_from_slice(&be32(sequence));
        });
        write_box(moof, b"traf", |traf| {
            write_box(traf, b"tfhd", |b| {
                // default-base-is-moof
                write_fullbox_header(b, 0, 0x020000);
                b.extend_from_slice(&be32(1)); // track_ID
            });
            write_box(traf, b"tfdt", |b| {
                write_fullbox_header(b, 1, 0);
                b.extend_from_slice(&be64(base_decode_time));
            });
            write_box(traf, b"trun", |b| {
                // data-offset-present | sample-duration-present |
                // sample-size-present | sample-flags-present
                write_fullbox_header(b, 0, 0x000001 | 0x000100 | 0x000200 | 0x000400);
                b.extend_from_slice(&be32(samples.len() as u32));
                // data_offset placeholder — relative to start of moof
                b.extend_from_slice(&be32(0));
                for s in samples {
                    b.extend_from_slice(&be32(s.duration));
                    b.extend_from_slice(&be32(s.data.len() as u32));
                    // sample_flags: keyframe has sample_depends_on=2, non-sync otherwise
                    let flags = if s.is_keyframe {
                        0x0200_0000u32
                    } else {
                        0x0101_0000u32
                    };
                    b.extend_from_slice(&be32(flags));
                }
            });
        });
    });

    // Patch data_offset = moof.len() + 8 (mdat header)
    let data_offset = (moof.len() as u32) + 8;
    // Locate the trun data_offset field: after version/flags(4) + sample_count(4)
    // We search for the trun box and patch.
    if let Some(pos) = find_trun_data_offset(&moof) {
        moof[pos..pos + 4].copy_from_slice(&be32(data_offset));
    }

    let mut out = moof;
    write_box(&mut out, b"mdat", |b| {
        let _ = b.write_all(&mdat_payload);
    });
    out
}

fn find_trun_data_offset(moof: &[u8]) -> Option<usize> {
    // Walk top-level boxes inside moof looking for traf/trun.
    let mut i = 8; // skip moof size+type
    while i + 8 <= moof.len() {
        let size = u32::from_be_bytes(moof[i..i + 4].try_into().ok()?) as usize;
        let typ = &moof[i + 4..i + 8];
        if typ == b"traf" {
            let mut j = i + 8;
            let end = i + size;
            while j + 8 <= end {
                let sz = u32::from_be_bytes(moof[j..j + 4].try_into().ok()?) as usize;
                let t = &moof[j + 4..j + 8];
                if t == b"trun" {
                    // version(1)+flags(3)+sample_count(4) then data_offset
                    return Some(j + 8 + 4 + 4);
                }
                if sz < 8 {
                    break;
                }
                j += sz;
            }
        }
        if size < 8 {
            break;
        }
        i += size;
    }
    None
}
