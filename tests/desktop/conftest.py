import pytest

# Before the import below, because pytest can only rewrite the asserts of a
# module it sees before Python loads it; the harness's asserts then explain
# their failures like those in the tests.
pytest.register_assert_rewrite("harness")

from harness import TONISH, Desktop


def pytest_sessionstart(session):
    if not TONISH.exists():
        pytest.exit(f"{TONISH}: build it first (cargo build --release)", returncode=1)


@pytest.fixture
def desktop():
    d = Desktop()
    yield d
    d.close()


@pytest.fixture
def desktops():
    """Starts a desktop for each call, with the arguments for tonish, for
    tests that compare runs."""
    made = []

    def start(*args):
        made.append(Desktop(*args))
        return made[-1]

    # For the report on a failure.
    start.made = made
    yield start
    for d in made:
        d.close()


@pytest.hookimpl(wrapper=True)
def pytest_runtest_makereport(item, call):
    """On a failure, while the desktop still runs: a screenshot for later, and
    the end of tonish's log in the report, because the cause is usually there
    and the desktop is gone once the test ends."""
    report = yield
    if report.when == "call" and report.failed:
        running = still_running(getattr(item, "funcargs", {}))
        for i, d in enumerate(running):
            name = item.name if len(running) == 1 else f"{item.name}-{i}"
            d.shot(name)
            tail = "\n".join(d.logged().splitlines()[-20:])
            report.sections.append((f"tonish log of {name}, last 20 lines", tail))
    return report


def still_running(funcargs):
    """A test may close a desktop it got from `desktops` before it fails,
    and a closed one has no log left to show."""
    ds = [funcargs["desktop"]] if "desktop" in funcargs else []
    ds += getattr(funcargs.get("desktops"), "made", [])
    return [d for d in ds if d.dir.exists()]
