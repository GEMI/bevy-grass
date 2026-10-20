"""Records the `walk` example and makes the README's film, docs/images/walk.webp.

    python tools/record_walk.py [--styles A,B,C] [--shot SECONDS] [--fps 15]
                                [--width 720] [--quality 30] [--out PATH]
                                [--keep-frames]

Builds `walk` in release, runs it with `--fixed-time --record`, so the wind
and the walk are the same on every run, and encodes every frame it saved
(30 for each second of game time) into an animated, looping WebP: at --fps
15 every second one, at 30 all of them. The frames go to target/walk, which
is emptied first and removed after unless --keep-frames is given.

--styles and --shot go to the example: the presets filmed, in order, and how
long each is shown. The film is as long as all the shots together.

Needs Pillow (`pip install pillow`). Recording takes a few minutes; the
window must stay open, and is best left uncovered.
"""

import argparse
import glob
import os
import shutil
import subprocess
import sys

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXE = ".exe" if os.name == "nt" else ""
WALK = os.path.join(ROOT, "target", "release", "examples", "walk" + EXE)
FRAMES = os.path.join(ROOT, "target", "walk")
# Frames the example saves per second of game time.
RECORDED_FPS = 30


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--styles", default="WILDFLOWER_MEADOW,RYE_AND_CORNFLOWERS,REED_MARSH")
    parser.add_argument("--shot", type=float, default=3.0)
    parser.add_argument("--fps", type=int, choices=(10, 15, 30), default=15)
    parser.add_argument("--width", type=int, default=720)
    # Dense, swaying grass compresses poorly: 30 keeps the 9 s film at 720 px
    # to 7.5 MB (40: 8.9 MB, 60: about 11 MB); finer blades blur a little.
    parser.add_argument("--quality", type=int, default=30)
    parser.add_argument("--out", default=os.path.join(ROOT, "docs", "images", "walk.webp"))
    parser.add_argument("--keep-frames", action="store_true")
    args = parser.parse_args()

    shots = len(args.styles.split(","))
    seconds = shots * args.shot
    # The example saves every second frame at 60 frames a second.
    record_frames = round(seconds * RECORDED_FPS) * 2

    subprocess.run(
        ["cargo", "build", "--release", "--example", "walk"], cwd=ROOT, check=True
    )
    shutil.rmtree(FRAMES, ignore_errors=True)
    subprocess.run(
        [
            WALK,
            "--fixed-time",
            "--record", FRAMES,
            "--record-frames", str(record_frames),
            "--styles", args.styles,
            "--shot", str(args.shot),
        ],
        cwd=ROOT,
        check=True,
    )

    paths = sorted(glob.glob(os.path.join(FRAMES, "*.png")))
    if not paths:
        sys.exit("walk saved no frames")
    paths = paths[:: RECORDED_FPS // args.fps]
    frames = []
    for path in paths:
        with Image.open(path) as frame:
            height = round(frame.height * args.width / frame.width)
            frames.append(frame.convert("RGB").resize((args.width, height), Image.LANCZOS))
    frames[0].save(
        args.out,
        save_all=True,
        append_images=frames[1:],
        duration=round(1000 / args.fps),
        loop=0,
        quality=args.quality,
        method=6,
    )
    size = os.path.getsize(args.out) / 1e6
    print(f"{args.out}: {len(frames)} frames, {len(frames) / args.fps:.1f} s, {size:.1f} MB")
    if not args.keep_frames:
        shutil.rmtree(FRAMES)


if __name__ == "__main__":
    main()
