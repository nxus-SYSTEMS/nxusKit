package httputil

import (
	"bufio"
	"errors"
	"io"
)

const (
	MaxUnaryResponseBodyBytes = 16 * 1024 * 1024
	MaxStreamRecordEventBytes = 4 * 1024 * 1024
	StreamDelimiterSlackBytes = 4
	initialScannerBufferBytes = 64 * 1024
)

var (
	ErrResponseBodyTooLarge = errors.New("response body exceeds 16777216 bytes")
	ErrStreamRecordTooLarge = errors.New("stream record/event exceeds 4194304 bytes")
)

func ReadBoundedBody(r io.Reader) ([]byte, error) {
	body, err := io.ReadAll(io.LimitReader(r, int64(MaxUnaryResponseBodyBytes)+1))
	if err != nil {
		return nil, err
	}
	if len(body) > MaxUnaryResponseBodyBytes {
		return nil, ErrResponseBodyTooLarge
	}
	return body, nil
}

func NewBoundedLineScanner(r io.Reader) *bufio.Scanner {
	scanner := bufio.NewScanner(r)
	scanner.Buffer(
		make([]byte, initialScannerBufferBytes),
		MaxStreamRecordEventBytes+StreamDelimiterSlackBytes,
	)
	scanner.Split(scanBoundedLines)
	return scanner
}

func scanBoundedLines(data []byte, atEOF bool) (advance int, token []byte, err error) {
	advance, token, err = bufio.ScanLines(data, atEOF)
	if err != nil {
		return advance, token, err
	}
	if token != nil {
		if len(token) > MaxStreamRecordEventBytes {
			return 0, nil, ErrStreamRecordTooLarge
		}
		return advance, token, nil
	}

	if len(data) > MaxStreamRecordEventBytes {
		// Permit an exact-limit record followed by a split CRLF delimiter.
		if len(data) == MaxStreamRecordEventBytes+1 && data[len(data)-1] == '\r' {
			return 0, nil, nil
		}
		return 0, nil, ErrStreamRecordTooLarge
	}
	return 0, nil, nil
}

type StreamEventBudget struct {
	used int
}

func (b *StreamEventBudget) Add(n int) error {
	if n < 0 || b.used > MaxStreamRecordEventBytes || n > MaxStreamRecordEventBytes-b.used {
		return ErrStreamRecordTooLarge
	}
	b.used += n
	return nil
}

func (b *StreamEventBudget) Reset() {
	b.used = 0
}
