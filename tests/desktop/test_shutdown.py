import signal

from harness import wait_for


def ended_cleanly(desktop):
    assert wait_for(lambda: desktop.tonish.poll() is not None), "tonish still runs"
    assert desktop.tonish.returncode == 0
    assert not desktop.socket.exists(), "the socket file was left behind"
    full = [(0, 0, 1920, 1080)] * 2
    assert wait_for(lambda: desktop.workspaces() == full), (
        f"the panels' room is still kept: {desktop.workspaces()}"
    )


def test_msg_quit_ends_tonish(desktop):
    desktop.wait_for_panels()
    sent = desktop.run_tonish("msg", "quit")
    assert sent.returncode == 0, sent.stderr
    # msg returns only once tonish has shut down, so the socket is gone now.
    assert not desktop.socket.exists(), "msg quit returned before tonish ended"
    ended_cleanly(desktop)
    again = desktop.run_tonish("msg", "quit")
    assert again.returncode == 1
    assert "no tonish answers" in again.stderr


def test_ctrl_c_ends_tonish_cleanly(desktop):
    desktop.wait_for_panels()
    desktop.tonish.send_signal(signal.SIGINT)
    ended_cleanly(desktop)
