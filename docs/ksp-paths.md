# Where KSP keeps blueprints

`ksp-share` resolves blueprints relative to the detected KSP install root.
The directory layout is:

```
<KSP root>/
└── Ships/
    ├── VAB/         # rockets built in the Vehicle Assembly Building
    └── SPH/         # planes built in the Spaceplane Hangar
```

## Detection order

1. The `KSP_ROOT` environment variable, if set.
2. Per-platform Steam install paths.
3. Common `~/KSP` and `~/KSP2` fallbacks.

The first directory that contains a `Ships/` subdirectory wins.

### Linux

- `~/.local/share/Steam/steamapps/common/Kerbal Space Program`
- `~/.steam/steam/steamapps/common/Kerbal Space Program`
- `~/.local/share/Steam/steamapps/common/Kerbal Space Program 2`

### macOS

- `~/Library/Application Support/Steam/steamapps/common/Kerbal Space Program`
- `~/Applications/Kerbal Space Program`

### Windows

- `C:\Program Files (x86)\Steam\steamapps\common\Kerbal Space Program`
- `C:\Program Files\Steam\steamapps\common\Kerbal Space Program`
- `C:\Program Files (x86)\Steam\steamapps\common\Kerbal Space Program 2`

## Overrides

Set `KSP_ROOT` to point at a custom install:

```sh
KSP_ROOT="/games/KSP" ksp-share list
```

## Inspecting detection

Run `ksp-share config` to see which paths were resolved. If the install
isn't detected the command prints the reason instead of failing.
