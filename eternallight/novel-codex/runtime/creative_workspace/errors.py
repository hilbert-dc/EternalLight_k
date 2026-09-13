class WorkspaceError(Exception):
    def __init__(
        self,
        code: str,
        message: str,
        status: int = 422,
        details: dict | None = None,
        recovery: str | None = None,
    ):
        super().__init__(message)
        self.code = code
        self.status = status
        self.details = details or {}
        self.recovery = recovery

    def payload(self) -> dict[str, object]:
        return {
            "error": {
                "code": self.code,
                "message": str(self),
                "details": self.details,
                "recovery": self.recovery,
            }
        }


class ConflictError(WorkspaceError):
    def __init__(
        self,
        code: str,
        message: str,
        details: dict | None = None,
        recovery: str | None = None,
    ):
        super().__init__(code, message, 409, details, recovery)


class NotFoundError(WorkspaceError):
    def __init__(self, message: str, details: dict | None = None):
        super().__init__("not_found", message, 404, details)


class UnauthorizedError(WorkspaceError):
    def __init__(self, message: str = "未授权的 Creative Runtime 请求"):
        super().__init__("unauthorized", message, 401)
