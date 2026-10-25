# Decoder regression fixtures

[Vendored decoder](../../../README.md#av1-decoder-portability)

The healthy fixtures are synthetic AV1 media retained for Capy's decoder
ownership tests. They contain no user artwork. The original encoder command and
version were not retained. Stream metadata is verified with FFmpeg's `ffprobe`;
the tests compare visible pixels across independent decoder contexts and retain
picture owners after close.

`invalid-tile.obu` is the fixed rejected-tile sample retained with the decoder
cleanup regression. Its bytes remain fixed; the tests require rejection, flush
and successful reuse with a healthy image.

| File | Bytes | Content | SHA-256 |
| --- | ---: | --- | --- |
| `moving-8bit.ivf` | 979 | Six 32 × 24 I420 8-bit frames | `9e67ed8a85aa6f291e2dcddc323dfb980b1d6a5115d4d1b3c3bfae216d0b6a2e` |
| `still-8bit.obu` | 146 | 16 × 16 I420 8-bit still | `02d57af3443d653a213f3107bdc8e5c2dc60dec4f663084b32a100ea6cf238f9` |
| `still-10bit.obu` | 150 | 16 × 16 I420 10-bit still | `6b0a8eb33473b3e3db696d0f9a5e35d035a148f3e99e4a5838de103614ac7bf4` |
| `still-12bit.obu` | 146 | 16 × 16 I420 12-bit still | `a5ea160aa217b9a4ece878355e6182046f87d76faba3ffe4688c169550b8280c` |
| `invalid-tile.obu` | 355 | Rejected tile data | `724f8e37b211abae18870ead803e340ecdb3d44aeab69db48ff96affcd43750f` |

Grain edge tests allocate their own initialized planes and explicit padded
references, covering I420, I422 and I444 at 8, 10 and 12 bits without relying on
encoded grain metadata.
