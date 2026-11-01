"""On this machine's GPU, which CI has none of: `just desktop -m hardware`.

The budgets leave room above what an RTX 4090 with NVIDIA's 615 driver takes,
so that a machine busy with other work passes: with a game running on the
same GPU, a resize took twice as long.
"""

import pytest
import timing

pytestmark = pytest.mark.hardware

# What a frame costs pleamar on the CPU alone, as its docs report (P11): the
# speed tonish is held to.
WARM_MS = 0.5
# Both happen once rather than in every frame of an animation, so they may take
# longer than a frame at 160 Hz, 6.25 ms. Measured: cold 1 to 3 ms, resized 1.5
# to 2.5 ms on an idle GPU and 5 to 7 ms beside a game.
COLD_MS = 5.0
RESIZE_MS = 10.0


@pytest.fixture
def desktop():
    """In place of conftest's, so that a failure shows this one's log."""
    d = timing.gpu_desktop()
    if d is None:
        pytest.skip("sway has no GPU to composite on here")
    yield d
    d.close()


def test_a_warm_redraw_takes_under_half_a_millisecond(desktop):
    redraws = timing.warm(desktop)
    assert timing.median(redraws) <= WARM_MS, [d.took for d in redraws]


def test_a_redraw_after_the_gpu_idled_takes_under_5_ms(desktop):
    redraws = timing.cold(desktop, times=2)
    assert timing.median(redraws) <= COLD_MS, [d.took for d in redraws]


def test_a_resize_takes_under_10_ms(desktop):
    redraws = timing.resizes(desktop)
    assert timing.median(redraws) <= RESIZE_MS, [d.took for d in redraws]
