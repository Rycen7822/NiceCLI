package cliproxy

import (
	"context"
	"net/http"
)

type websocketGateway interface {
	Path() string
	Handler() http.Handler
	Stop(context.Context) error
}
