package httputil

import (
	"bytes"
	"errors"
	"testing"
	"testing/iotest"
)

func TestResourceBoundsFrozenLimits(t *testing.T) {
	if MaxUnaryResponseBodyBytes != 16*1024*1024 {
		t.Fatalf("unary limit = %d", MaxUnaryResponseBodyBytes)
	}
	if MaxStreamRecordEventBytes != 4*1024*1024 {
		t.Fatalf("stream limit = %d", MaxStreamRecordEventBytes)
	}
	if StreamDelimiterSlackBytes != 4 {
		t.Fatalf("delimiter slack = %d", StreamDelimiterSlackBytes)
	}
}

func TestResourceBoundedBodyExactLimit(t *testing.T) {
	input := bytes.Repeat([]byte{'x'}, MaxUnaryResponseBodyBytes)
	got, err := ReadBoundedBody(bytes.NewReader(input))
	if err != nil {
		t.Fatalf("exact limit failed: %v", err)
	}
	if len(got) != MaxUnaryResponseBodyBytes {
		t.Fatalf("body length = %d", len(got))
	}
}

func TestResourceBoundedBodyOneByteOver(t *testing.T) {
	input := bytes.Repeat([]byte{'x'}, MaxUnaryResponseBodyBytes+1)
	_, err := ReadBoundedBody(bytes.NewReader(input))
	if !errors.Is(err, ErrResponseBodyTooLarge) {
		t.Fatalf("expected ErrResponseBodyTooLarge, got %v", err)
	}
}

func TestResourceBoundedLineScannerExactLF(t *testing.T) {
	input := append(bytes.Repeat([]byte{'x'}, MaxStreamRecordEventBytes), '\n')
	scanner := NewBoundedLineScanner(bytes.NewReader(input))
	if !scanner.Scan() {
		t.Fatalf("exact LF record failed: %v", scanner.Err())
	}
	if len(scanner.Bytes()) != MaxStreamRecordEventBytes {
		t.Fatalf("record length = %d", len(scanner.Bytes()))
	}
	if scanner.Scan() {
		t.Fatal("unexpected second record")
	}
	if err := scanner.Err(); err != nil {
		t.Fatalf("scanner error: %v", err)
	}
}

func TestResourceBoundedLineScannerExactCRLF(t *testing.T) {
	input := append(bytes.Repeat([]byte{'x'}, MaxStreamRecordEventBytes), '\r', '\n')
	scanner := NewBoundedLineScanner(bytes.NewReader(input))
	if !scanner.Scan() {
		t.Fatalf("exact CRLF record failed: %v", scanner.Err())
	}
	if len(scanner.Bytes()) != MaxStreamRecordEventBytes {
		t.Fatalf("record length = %d", len(scanner.Bytes()))
	}
}

func TestResourceBoundedLineScannerOneByteOver(t *testing.T) {
	input := append(bytes.Repeat([]byte{'x'}, MaxStreamRecordEventBytes+1), '\n')
	scanner := NewBoundedLineScanner(bytes.NewReader(input))
	if scanner.Scan() {
		t.Fatal("one-byte-over record unexpectedly succeeded")
	}
	if !errors.Is(scanner.Err(), ErrStreamRecordTooLarge) {
		t.Fatalf("expected ErrStreamRecordTooLarge, got %v", scanner.Err())
	}
}

func TestResourceBoundedLineScannerFinalRecordWithoutNewline(t *testing.T) {
	scanner := NewBoundedLineScanner(bytes.NewReader([]byte("final")))
	if !scanner.Scan() || scanner.Text() != "final" {
		t.Fatalf("final record failed: text=%q err=%v", scanner.Text(), scanner.Err())
	}
	if scanner.Scan() {
		t.Fatal("unexpected second record")
	}
	if err := scanner.Err(); err != nil {
		t.Fatalf("scanner error: %v", err)
	}
}

func TestResourceBoundedLineScannerFragmentedUTF8(t *testing.T) {
	input := []byte("alpha € omega\n")
	scanner := NewBoundedLineScanner(iotest.OneByteReader(bytes.NewReader(input)))
	if !scanner.Scan() {
		t.Fatalf("fragmented UTF-8 failed: %v", scanner.Err())
	}
	if scanner.Text() != "alpha € omega" {
		t.Fatalf("unexpected text %q", scanner.Text())
	}
}

func TestResourceStreamEventBudgetExactAndOver(t *testing.T) {
	var budget StreamEventBudget
	if err := budget.Add(MaxStreamRecordEventBytes - 1); err != nil {
		t.Fatalf("initial budget add failed: %v", err)
	}
	if err := budget.Add(1); err != nil {
		t.Fatalf("exact budget failed: %v", err)
	}
	if err := budget.Add(1); !errors.Is(err, ErrStreamRecordTooLarge) {
		t.Fatalf("expected ErrStreamRecordTooLarge, got %v", err)
	}
}

func TestResourceStreamEventBudgetReset(t *testing.T) {
	var budget StreamEventBudget
	if err := budget.Add(MaxStreamRecordEventBytes); err != nil {
		t.Fatalf("exact budget failed: %v", err)
	}
	budget.Reset()
	if err := budget.Add(MaxStreamRecordEventBytes); err != nil {
		t.Fatalf("budget after reset failed: %v", err)
	}
}
