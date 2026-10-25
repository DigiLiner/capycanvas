# Synthetic TIFF fixtures

`jpeg-gray-alpha.tif` is a 24 × 16 gray image whose alpha rises from 0 at the
left edge to almost 255 at the right, stored as one JPEG-compressed gray+alpha
strip. ImageMagick 7.1.2-31 wrote it with:

```sh
magick -size 24x16 gradient:gray20-gray80 -alpha set -channel A -fx 'i/w' +channel \
  -type GrayscaleAlpha -compress JPEG -quality 95 jpeg-gray-alpha.tif
```

SHA-256: `4c28c6646a9a27e5c22c4a603786fba5b236c5554d195f83bb230508f386322e`
