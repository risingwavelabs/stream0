class Stream0Error(Exception):
    """Base exception for stream0 SDK."""

    def __init__(self, message, status_code=None, response=None):
        super().__init__(message)
        self.status_code = status_code
        self.response = response


class AuthenticationError(Stream0Error):
    """Raised when API key is missing or invalid."""
    pass


class NotFoundError(Stream0Error):
    """Raised when a resource (topic, message) is not found."""
    pass


class TimeoutError(Stream0Error):
    """Raised when a request-reply times out waiting for a response."""
    pass


class ServerError(Stream0Error):
    """Raised when the server returns a 5xx error."""
    pass
