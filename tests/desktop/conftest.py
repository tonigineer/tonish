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

    yield start
    for d in made:
        d.close()


@pytest.hookimpl(wrapper=True)
def pytest_runtest_makereport(item, call):
    """On a failure, while the desktop still runs: a screenshot for later, and
    the end of tonish's log in the report, because the cause is usually there
    and the desktop is gone once the test ends."""
    report = yield
    d = getattr(item, "funcargs", {}).get("desktop")
    if report.when == "call" and report.failed and d is not None:
        d.shot(item.name)
        tail = "\n".join(d.logged().splitlines()[-20:])
        report.sections.append(("tonish log, last 20 lines", tail))
    return report
