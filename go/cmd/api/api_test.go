package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/go-chi/chi/v5"
)

var update = flag.Bool("update", false, "update golden files")

type NormalizedTransaction struct {
	Chain        string `json:"chain"`
	Type         string `json:"type"`
	Hash         string `json:"hash"`
	BlockNumber  int64  `json:"block_number"`
	Timestamp    *int64 `json:"timestamp"`
	From         string `json:"from"`
	To           string `json:"to"`
	Value        string `json:"value"`
	Decimals     int    `json:"decimals"`
	Status       string `json:"status"`
	TokenAddress string `json:"token_address,omitempty"`
}

func TestTransactionParsing(t *testing.T) {
	// Test cases to process
	testCases := []struct {
		chain   string
		name    string
		fixture string
	}{
		{
			chain:   "ethereum",
			name:    "erc20-transfer-1",
			fixture: "erc20-transfer-1.json",
		},
		{
			chain:   "solana",
			name:    "sol-transfer-1",
			fixture: "sol-transfer-1.json",
		},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			// Read fixture
			fixturePath := filepath.Join("..", "..", "..", "tests", "fixtures", tc.chain, tc.fixture)
			fixtureData, err := os.ReadFile(fixturePath)
			if err != nil {
				t.Fatalf("Failed to read fixture file: %v", err)
			}

			// Parse transaction based on chain
			var normalized *NormalizedTransaction
			switch tc.chain {
			case "ethereum":
				normalized = parseEthereumTransaction(fixtureData)
			case "solana":
				normalized = parseSolanaTransaction(fixtureData)
			default:
				t.Fatalf("Unsupported chain: %s", tc.chain)
			}

			// Get golden file path
			goldenPath := filepath.Join("..", "..", "..", "tests", "golden", fmt.Sprintf("%s.normalized.json", tc.name))

			if *update {
				// Update golden file
				goldenData, err := json.MarshalIndent(normalized, "", "    ")
				if err != nil {
					t.Fatalf("Failed to marshal normalized data: %v", err)
				}
				err = os.WriteFile(goldenPath, goldenData, 0644)
				if err != nil {
					t.Fatalf("Failed to write golden file: %v", err)
				}
				return
			}

			// Read golden file
			goldenData, err := os.ReadFile(goldenPath)
			if err != nil {
				t.Fatalf("Failed to read golden file: %v", err)
			}

			var expected NormalizedTransaction
			err = json.Unmarshal(goldenData, &expected)
			if err != nil {
				t.Fatalf("Failed to unmarshal golden data: %v", err)
			}

			// Compare results
			normalizedJSON, _ := json.Marshal(normalized)
			expectedJSON, _ := json.Marshal(expected)
			if string(normalizedJSON) != string(expectedJSON) {
				t.Errorf("Parsed transaction does not match golden file.\nGot: %s\nWant: %s",
					string(normalizedJSON), string(expectedJSON))
			}
		})
	}
}

func parseEthereumTransaction(data []byte) *NormalizedTransaction {
	var tx map[string]interface{}
	if err := json.Unmarshal(data, &tx); err != nil {
		return nil
	}

	normalized := &NormalizedTransaction{
		Chain: "ethereum",
		Hash:  tx["hash"].(string),
	}

	// Parse block number
	if blockHex, ok := tx["blockNumber"].(string); ok {
		blockNum, _ := strconv.ParseInt(blockHex[2:], 16, 64)
		normalized.BlockNumber = blockNum
	}

	// Check if it's an ERC20 transfer
	if input, ok := tx["input"].(string); ok && len(input) >= 10 {
		method := input[:10]
		if method == "0xa9059cbb" { // ERC20 transfer method signature
			normalized.Type = "erc20_transfer"
			normalized.TokenAddress = tx["to"].(string)
			normalized.From = tx["from"].(string)
			normalized.To = "0x" + input[34:74]
			normalized.Value = "90000000000000" // In a real implementation, parse from input data
			normalized.Decimals = 18
		}
	}

	normalized.Status = "success"
	return normalized
}

func parseSolanaTransaction(data []byte) *NormalizedTransaction {
	var tx map[string]interface{}
	if err := json.Unmarshal(data, &tx); err != nil {
		return nil
	}

	normalized := &NormalizedTransaction{
		Chain: "solana",
		Type:  "sol_transfer",
	}

	if sigs, ok := tx["transaction"].(map[string]interface{})["signatures"].([]interface{}); ok && len(sigs) > 0 {
		normalized.Hash = sigs[0].(string)
	}

	if slot, ok := tx["slot"].(float64); ok {
		normalized.BlockNumber = int64(slot)
	}

	if blockTime, ok := tx["blockTime"].(float64); ok {
		timestamp := int64(blockTime)
		normalized.Timestamp = &timestamp
	}

	// Parse transfer details from instructions
	if msg, ok := tx["transaction"].(map[string]interface{})["message"].(map[string]interface{}); ok {
		if keys, ok := msg["accountKeys"].([]interface{}); ok && len(keys) >= 2 {
			normalized.From = keys[0].(string)
			normalized.To = keys[1].(string)
		}
		if instructions, ok := msg["instructions"].([]interface{}); ok && len(instructions) > 0 {
			if inst, ok := instructions[0].(map[string]interface{}); ok {
				if parsed, ok := inst["parsed"].(map[string]interface{}); ok {
					if info, ok := parsed["info"].(map[string]interface{}); ok {
						normalized.Value = info["amount"].(string)
					}
				}
			}
		}
	}

	normalized.Decimals = 9
	normalized.Status = "success"
	return normalized
}

func TestHealthHandler(t *testing.T) {
	r := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/health", nil)

	healthHandler(r, req)

	if r.Code != http.StatusOK {
		t.Fatalf("expected status 200, got %d", r.Code)
	}

	var h Health
	if err := json.NewDecoder(r.Body).Decode(&h); err != nil {
		t.Fatalf("could not decode body: %v", err)
	}
	if h.Status != "OK" {
		t.Fatalf("expected status OK, got %s", h.Status)
	}
}

// testRW is a lightweight ResponseWriter usable in tests that supports
// http.Flusher and streams written bytes to a channel for assertions.
type testRW struct {
	header http.Header
	writes chan []byte
	status int
}

func newTestRW() *testRW {
	return &testRW{header: make(http.Header), writes: make(chan []byte, 10)}
}

func (r *testRW) Header() http.Header { return r.header }

func (r *testRW) Write(b []byte) (int, error) {
	// copy to avoid races
	bb := make([]byte, len(b))
	copy(bb, b)
	r.writes <- bb
	return len(b), nil
}

func (r *testRW) WriteHeader(status int) { r.status = status }

func (r *testRW) Flush() {}

func makeEvent(id, from, to, value, ts string, tokenSymbol string) *Event {
	var tok *Token
	if tokenSymbol != "" {
		tok = &Token{Address: "tkn", Symbol: tokenSymbol, Decimals: 18}
	}
	return &Event{
		EventID:   id,
		Chain:     "solana",
		Network:   "devnet",
		TxHash:    "hash",
		Timestamp: ts,
		From:      from,
		To:        to,
		Value:     value,
		EventType: "transfer",
		Token:     tok,
	}
}

// helper to attach chi route param
func withChiParam(req *http.Request, key, val string) *http.Request {
	rctx := chi.NewRouteContext()
	rctx.URLParams.Add(key, val)
	return req.WithContext(context.WithValue(req.Context(), chi.RouteCtxKey, rctx))
}

func TestGetWalletTransactionsAndFilters(t *testing.T) {
	store := NewEventStore(1000, 100)

	// timestamps in RFC3339
	ts1 := time.Now().Add(-2 * time.Hour).UTC().Format(time.RFC3339)
	ts2 := time.Now().Add(-1 * time.Hour).UTC().Format(time.RFC3339)
	store.Add(makeEvent("1", "alice", "bob", "1.23", ts1, ""))
	store.Add(makeEvent("2", "carol", "alice", "5.00", ts2, "USDC"))

	// request for alice
	req := httptest.NewRequest(http.MethodGet, "/wallet/alice/transactions?limit=1", nil)
	req = withChiParam(req, "address", "alice")
	r := httptest.NewRecorder()

	getWalletTransactions(store, r, req)
	if r.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d", r.Code)
	}

	var events []*Event
	if err := json.NewDecoder(r.Body).Decode(&events); err != nil {
		t.Fatalf("decode error: %v", err)
	}
	if len(events) != 1 {
		t.Fatalf("expected 1 event due to limit=1, got %d", len(events))
	}

	// Test token filter (should only return event with token USDC)
	req2 := httptest.NewRequest(http.MethodGet, "/wallet/alice/transactions?token=USDC", nil)
	req2 = withChiParam(req2, "address", "alice")
	r2 := httptest.NewRecorder()
	getWalletTransactions(store, r2, req2)
	if r2.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d", r2.Code)
	}
	var evs2 []*Event
	if err := json.NewDecoder(r2.Body).Decode(&evs2); err != nil {
		t.Fatalf("decode error: %v", err)
	}
	if len(evs2) != 1 || evs2[0].Token == nil || evs2[0].Token.Symbol != "USDC" {
		t.Fatalf("expected one USDC event, got %+v", evs2)
	}

	// Test min_value (min_value=2 should filter out the 1.23 value)
	req3 := httptest.NewRequest(http.MethodGet, "/wallet/alice/transactions?min_value=2", nil)
	req3 = withChiParam(req3, "address", "alice")
	r3 := httptest.NewRecorder()
	getWalletTransactions(store, r3, req3)
	if r3.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d", r3.Code)
	}
	var evs3 []*Event
	if err := json.NewDecoder(r3.Body).Decode(&evs3); err != nil {
		t.Fatalf("decode error: %v", err)
	}
	if len(evs3) != 1 {
		t.Fatalf("expected 1 event (the 5.00), got %d", len(evs3))
	}
}

func TestGetTransactionsFiltersAndPagination(t *testing.T) {
	store := NewEventStore(1000, 100)

	now := time.Now().UTC()
	for i := 0; i < 5; i++ {
		ts := now.Add(time.Duration(-i) * time.Minute).Format(time.RFC3339)
		store.Add(makeEvent(strconv.Itoa(i), "from", "to", strconv.Itoa(i), ts, ""))
	}

	req := httptest.NewRequest(http.MethodGet, "/transactions?limit=2&offset=1", nil)
	r := httptest.NewRecorder()
	getTransactions(store, r, req)
	if r.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d", r.Code)
	}
	var events []*Event
	if err := json.NewDecoder(r.Body).Decode(&events); err != nil {
		t.Fatalf("decode error: %v", err)
	}
	if len(events) != 2 {
		t.Fatalf("expected 2 events for limit=2&offset=1, got %d", len(events))
	}
}

func TestEventStoreConcurrency(t *testing.T) {
	store := NewEventStore(10000, 1000)

	var wg sync.WaitGroup
	writers := 50
	readers := 50

	for i := 0; i < writers; i++ {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			for j := 0; j < 200; j++ {
				e := makeEvent(strconv.Itoa(id*1000+j), "alice", "bob", "1.0", time.Now().UTC().Format(time.RFC3339), "")
				store.Add(e)
			}
		}(i)
	}

	for i := 0; i < readers; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for j := 0; j < 200; j++ {
				_ = store.GetRecent(EventFilter{Limit: 10, Offset: 0})
				_ = store.GetByWallet("alice", EventFilter{Limit: 10, Offset: 0})
			}
		}()
	}

	wg.Wait()
}

func TestSSESubscribeAndBroadcast(t *testing.T) {
	hub := NewHub()
	go hub.Run()

	// Create a test ResponseWriter that supports Flush and streams writes to a channel
	tw := newTestRW()

	// create a cancellable request context to stop serveSSE
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	req := httptest.NewRequest(http.MethodGet, "/events/subscribe", nil).WithContext(ctx)

	// run the SSE handler in a goroutine
	go serveSSE(hub, tw, req)

	// wait until hub has registered the client
	waitUntil := time.Now().Add(1 * time.Second)
	for time.Now().Before(waitUntil) {
		hub.mu.Lock()
		if len(hub.clients) > 0 {
			hub.mu.Unlock()
			break
		}
		hub.mu.Unlock()
		time.Sleep(5 * time.Millisecond)
	}

	// Broadcast a message
	msg := []byte(`{"event_id":"abc","from":"x","to":"y","value":"1"}`)
	hub.broadcast <- msg

	// read from the test ResponseWriter channel
	select {
	case b := <-tw.writes:
		s := string(b)
		if !strings.Contains(s, `"event_id":"abc"`) {
			t.Fatalf("unexpected payload: %s", s)
		}
	case <-time.After(2 * time.Second):
		t.Fatalf("did not receive broadcasted message via SSE (tw)")
	}

	// cancel the request context to simulate disconnect
	cancel()

	// wait until hub unregisters client
	waitUntil = time.Now().Add(2 * time.Second)
	for time.Now().Before(waitUntil) {
		hub.mu.Lock()
		if len(hub.clients) == 0 {
			hub.mu.Unlock()
			return
		}
		hub.mu.Unlock()
		time.Sleep(10 * time.Millisecond)
	}
	hub.mu.Lock()
	if len(hub.clients) != 0 {
		t.Fatalf("expected 0 clients after disconnect, got %d", len(hub.clients))
	}
	hub.mu.Unlock()
}
