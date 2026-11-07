#!/usr/bin/env python3
"""Stage native bundles or assemble a complete, signed updater manifest."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import shutil

PLATFORMS = {
    'macos-arm64': ('darwin-aarch64', {'dmg': 'dmg/*.dmg', 'app.tar.gz': 'macos/*.app.tar.gz'}),
    'macos-x64': ('darwin-x86_64', {'dmg': 'dmg/*.dmg', 'app.tar.gz': 'macos/*.app.tar.gz'}),
    'linux-arm64': ('linux-aarch64', {'AppImage': 'appimage/*.AppImage', 'deb': 'deb/*.deb'}),
    'linux-x64': ('linux-x86_64', {'AppImage': 'appimage/*.AppImage', 'deb': 'deb/*.deb'}),
}


def stage(platform, bundle, output):
    output.mkdir(parents=True, exist_ok=True)
    _, formats = PLATFORMS[platform]
    for extension, pattern in formats.items():
        matches = list(bundle.glob(pattern))
        if len(matches) != 1:
            raise ValueError(f'expected exactly one {pattern}, found {len(matches)}')
        source = matches[0]
        target = output / f'Herdstead-{platform}.{extension}'
        shutil.copyfile(source, target)
        if extension in ('app.tar.gz', 'AppImage'):
            signature = Path(str(source) + '.sig')
            if not signature.read_text().strip():
                raise ValueError(f'empty update signature: {signature}')
            shutil.copyfile(signature, Path(str(target) + '.sig'))


def assemble(output, tag, repository):
    platforms = {}
    expected = set()
    for platform, (key, formats) in PLATFORMS.items():
        for extension in formats:
            name = f'Herdstead-{platform}.{extension}'
            expected.add(name)
            path = output / name
            if not path.is_file() or not path.stat().st_size:
                raise ValueError(f'missing or empty installer: {name}')
            if extension in ('app.tar.gz', 'AppImage'):
                signature = name + '.sig'
                expected.add(signature)
                value = (output / signature).read_text().strip()
                if not value:
                    raise ValueError(f'empty signature: {signature}')
                platforms[key] = {'signature': value, 'url': f'https://github.com/{repository}/releases/download/{tag}/{name}'}
    actual = {path.name for path in output.iterdir() if path.is_file()}
    if actual != expected:
        raise ValueError(f'unexpected or missing assets: {actual ^ expected}')
    manifest = {'version': tag.removeprefix('v'), 'notes': f'Herdstead {tag}',
                'pub_date': datetime.now(timezone.utc).isoformat(), 'platforms': platforms}
    (output / 'latest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    checksums = []
    for path in sorted(output.iterdir()):
        checksums.append(f'{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n')
    (output / 'SHA256SUMS').write_text(''.join(checksums))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    package = commands.add_parser('stage')
    package.add_argument('platform', choices=PLATFORMS)
    package.add_argument('bundle', type=Path)
    package.add_argument('output', type=Path)
    manifest = commands.add_parser('assemble')
    manifest.add_argument('output', type=Path)
    manifest.add_argument('tag')
    manifest.add_argument('repository')
    args = parser.parse_args()
    if args.command == 'stage':
        stage(args.platform, args.bundle, args.output)
    else:
        assemble(args.output, args.tag, args.repository)
