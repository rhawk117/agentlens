"""Core signals."""


class Signal:
    def __init__(self, providing_args=None):
        self.providing_args = providing_args or []
        self.receivers = []

    def connect(self, receiver):
        self.receivers.append(receiver)

    def send(self, sender, **named):
        return [(r, r(sender=sender, **named)) for r in self.receivers]


request_started = Signal()
request_finished = Signal()
got_request_exception = Signal()
setting_changed = Signal()
