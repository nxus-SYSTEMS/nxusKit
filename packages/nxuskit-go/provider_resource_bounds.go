package nxuskit

import (
	"bytes"
	"encoding/json"
	"net/http"

	"github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go/internal/httputil"
)

func decodeProviderJSON(body []byte, target any) error {
	return json.NewDecoder(bytes.NewReader(body)).Decode(target)
}

func decodeBoundedProviderResponse(
	resp *http.Response,
	providerName string,
	decodeMessage string,
	target any,
) error {
	body, err := httputil.ReadBoundedBody(resp.Body)
	if err != nil {
		return NewProviderError(providerName, err.Error(), 0, err)
	}
	if err := decodeProviderJSON(body, target); err != nil {
		return NewProviderError(providerName, decodeMessage, 0, err)
	}
	return nil
}
