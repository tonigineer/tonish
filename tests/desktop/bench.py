"""`just bench-desktop`: how long tonish takes to draw on this machine's GPU,
warm, cold and resized, in a headless sway of its own."""

import argparse
import sys

import timing


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--warm", type=int, default=20, help="warm redraws")
    parser.add_argument("--cold", type=int, default=3, help="cold redraws")
    parser.add_argument(
        "--idle", type=float, default=10.0, help="seconds idle before a cold one"
    )
    parser.add_argument("--resizes", type=int, default=10, help="resizes")
    args = parser.parse_args()

    desktop = timing.gpu_desktop()
    if desktop is None:
        sys.exit("bench-desktop: sway has no GPU to composite on here")
    try:
        print(f"{timing.device(desktop)}, two 3840×2160 monitors at scale 2")
        cases = [
            ("warm redraw", timing.warm(desktop, times=args.warm)),
            ("resize", timing.resizes(desktop, times=args.resizes)),
            ("cold redraw", timing.cold(desktop, times=args.cold, idle=args.idle)),
        ]
    finally:
        desktop.close()
    print(f"{'':12}{'redraws':>9}{'median':>11}{'slowest':>11}{'waited':>11}")
    for name, redraws in cases:
        waited = max(d.waited for d in redraws)
        print(
            f"{name:12}{len(redraws):>9}"
            f"{timing.median(redraws):>8.3f} ms{timing.worst(redraws):>8.3f} ms"
            f"{waited:>8.3f} ms"
        )


if __name__ == "__main__":
    main()
