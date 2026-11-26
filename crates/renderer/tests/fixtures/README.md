# Render fixtures

- Source Code Pro Regular is the unmodified Adobe font from the Fedora
  `adobe-source-code-pro-fonts` package. See `SourceCodePro-LICENSE.txt`.
- Droid Sans Japanese is from the Fedora `google-droid-sans-fonts` package.
  It was subset to U+754C with FontTools 4.68.0 using
  `--unicodes=U+754C --name-IDs=* --name-legacy`. See
  `DroidSansJapanese-NOTICE.txt`.

The renderer test loads these fonts in an empty font database.
