package main

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/go-redis/redis/v8"
	"github.com/jackc/pgx/v5/pgxpool"
	log "github.com/sirupsen/logrus"
)

const (
	maxEvents          = 1000
	maxEventsPerWallet = 100
)

type Health struct {
	Status string `json:"status"`
}

type Token struct {
	Address  string `json:"address"`
	Symbol   string `json:"symbol"`
	Decimals uint8  `json:"decimals"`
}

type Event struct {
	EventID   string  `json:"event_id"`
	Chain     string  `json:"chain"`
	Network   string  `json:"network"`
	TxHash    string  `json:"tx_hash"`
	Timestamp string  `json:"timestamp"`
	From      string  `json:"from"`
	To        string  `json:"to"`
	Value     string  `json:"value"`
	EventType string  `json:"event_type"`
	Slot      *uint64 `json:"slot,omitempty"`
	Token     *Token  `json:"token,omitempty"`
}

// EventFilter struct to hold all filter, sort, and pagination parameters
type EventFilter struct {
	Chain     string
	Token     string
	From      string
	To        string
	MinValue  float64
	StartTime *time.Time
	EndTime   *time.Time
	SortBy    string
	SortOrder string
	Limit     int
	Offset    int
}

type EventStore struct {
	mu                 sync.RWMutex
	events             []*Event
	eventsByWallet     map[string][]*Event
	maxTotalEvents     int
	maxEventsPerWallet int
	db                 *pgxpool.Pool
}

func NewEventStore(maxTotalEvents, maxEventsPerWallet int) *EventStore {
	return &EventStore{
		events:             make([]*Event, 0),
		eventsByWallet:     make(map[string][]*Event),
		maxTotalEvents:     maxTotalEvents,
		maxEventsPerWallet: maxEventsPerWallet,
	}
}

func (s *EventStore) AttachDB(db *pgxpool.Pool) {
	s.db = db
}

func (s *EventStore) Add(event *Event) {
	s.mu.Lock()
	defer s.mu.Unlock()

	// Normalize addresses to lowercase for case-insensitive lookups
	event.From = strings.ToLower(event.From)
	event.To = strings.ToLower(event.To)

	// Add to global list and trim
	s.events = append([]*Event{event}, s.events...)
	if len(s.events) > s.maxTotalEvents {
		s.events = s.events[:s.maxTotalEvents]
	}

	// Add to "from" wallet and trim
	fromEvents := s.eventsByWallet[event.From]
	fromEvents = append([]*Event{event}, fromEvents...)
	if len(fromEvents) > s.maxEventsPerWallet {
		fromEvents = fromEvents[:s.maxEventsPerWallet]
	}
	s.eventsByWallet[event.From] = fromEvents

	// Add to "to" wallet and trim
	toEvents := s.eventsByWallet[event.To]
	toEvents = append([]*Event{event}, toEvents...)
	if len(toEvents) > s.maxEventsPerWallet {
		toEvents = toEvents[:s.maxEventsPerWallet]
	}
	s.eventsByWallet[event.To] = toEvents
}

type Hub struct {
	clients    map[chan []byte]struct{}
	register   chan chan []byte
	unregister chan chan []byte
	broadcast  chan []byte
	mu         sync.Mutex
}

func (s *EventStore) GetByWallet(address string, filter EventFilter) []*Event {
	// If DB is attached, read from DB for persistence/idempotency
	if s.db != nil {
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()

		// Build simple query ordering by created_at desc (server-side timestamp)
		// We intentionally keep filtering minimal to avoid complexity.
		q := `
			SELECT event_id, chain, network, tx_hash, timestamp, from_addr, to_addr, value, event_type, slot,
				   token_address, token_symbol, token_decimals
			FROM events
			WHERE (LOWER(from_addr) = $1 OR LOWER(to_addr) = $1)
		`
		args := []interface{}{strings.ToLower(address)}
		idx := 2
		if filter.Chain != "" {
			q += fmt.Sprintf(" AND chain = $%d", idx)
			args = append(args, filter.Chain)
			idx++
		}
		if filter.Token != "" {
			q += fmt.Sprintf(" AND token_symbol = $%d", idx)
			args = append(args, filter.Token)
			idx++
		}
		if filter.From != "" {
			q += fmt.Sprintf(" AND LOWER(from_addr) = $%d", idx)
			args = append(args, strings.ToLower(filter.From))
			idx++
		}
		if filter.To != "" {
			q += fmt.Sprintf(" AND LOWER(to_addr) = $%d", idx)
			args = append(args, strings.ToLower(filter.To))
			idx++
		}
		// Order and paginate using created_at for stability
		q += fmt.Sprintf(" ORDER BY created_at DESC LIMIT $%d OFFSET $%d", idx, idx+1)
		if filter.Limit == 0 {
			filter.Limit = 50
		}
		args = append(args, filter.Limit, filter.Offset)

		rows, err := s.db.Query(ctx, q, args...)
		if err != nil {
			log.WithError(err).Warn("db query failed; falling back to in-memory")
		} else {
			defer rows.Close()
			out := make([]*Event, 0)
			for rows.Next() {
				var ev Event
				var slot *int64
				var tokAddr, tokSym *string
				var tokDec *int32
				if err := rows.Scan(&ev.EventID, &ev.Chain, &ev.Network, &ev.TxHash, &ev.Timestamp,
					&ev.From, &ev.To, &ev.Value, &ev.EventType, &slot, &tokAddr, &tokSym, &tokDec); err != nil {
					log.WithError(err).Warn("db scan failed")
					continue
				}
				if slot != nil {
					s := uint64(*slot)
					ev.Slot = &s
				}
				if tokAddr != nil || tokSym != nil || tokDec != nil {
					ev.Token = &Token{Address: getOrEmpty(tokAddr), Symbol: getOrEmpty(tokSym)}
					if tokDec != nil {
						ev.Token.Decimals = uint8(*tokDec)
					}
				}
				out = append(out, &ev)
			}
			return out
		}
	}

	// Fallback: in-memory filtering (legacy)
	s.mu.RLock()
	defer s.mu.RUnlock()

	var filteredEvents []*Event
	for _, event := range s.eventsByWallet[address] {
		if filter.Chain != "" && event.Chain != filter.Chain {
			continue
		}
		if filter.Token != "" && (event.Token == nil || event.Token.Symbol != filter.Token) {
			continue
		}
		if filter.From != "" && event.From != filter.From {
			continue
		}
		if filter.To != "" && event.To != filter.To {
			continue
		}
		if filter.MinValue > 0 {
			if val, err := strconv.ParseFloat(event.Value, 64); err == nil {
				if val < filter.MinValue {
					continue
				}
			}
		}
		filteredEvents = append(filteredEvents, event)
	}

	if filter.Offset >= len(filteredEvents) {
		return []*Event{}
	}
	end := filter.Offset + filter.Limit
	if end > len(filteredEvents) {
		end = len(filteredEvents)
	}
	return filteredEvents[filter.Offset:end]
}

func (s *EventStore) GetRecent(filter EventFilter) []*Event {
	if s.db != nil {
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()

		q := `
			SELECT event_id, chain, network, tx_hash, timestamp, from_addr, to_addr, value, event_type, slot,
				   token_address, token_symbol, token_decimals
			FROM events
			WHERE 1=1
		`
		args := []interface{}{}
		idx := 1
		if filter.Chain != "" {
			q += fmt.Sprintf(" AND chain = $%d", idx)
			args = append(args, filter.Chain)
			idx++
		}
		if filter.Token != "" {
			q += fmt.Sprintf(" AND token_symbol = $%d", idx)
			args = append(args, filter.Token)
			idx++
		}
		if filter.From != "" {
			q += fmt.Sprintf(" AND LOWER(from_addr) = $%d", idx)
			args = append(args, strings.ToLower(filter.From))
			idx++
		}
		if filter.To != "" {
			q += fmt.Sprintf(" AND LOWER(to_addr) = $%d", idx)
			args = append(args, strings.ToLower(filter.To))
			idx++
		}
		// Order by created_at desc for recency
		q += fmt.Sprintf(" ORDER BY created_at DESC LIMIT $%d OFFSET $%d", idx, idx+1)
		if filter.Limit == 0 {
			filter.Limit = 50
		}
		args = append(args, filter.Limit, filter.Offset)

		rows, err := s.db.Query(ctx, q, args...)
		if err == nil {
			defer rows.Close()
			out := make([]*Event, 0)
			for rows.Next() {
				var ev Event
				var slot *int64
				var tokAddr, tokSym *string
				var tokDec *int32
				if err := rows.Scan(&ev.EventID, &ev.Chain, &ev.Network, &ev.TxHash, &ev.Timestamp,
					&ev.From, &ev.To, &ev.Value, &ev.EventType, &slot, &tokAddr, &tokSym, &tokDec); err != nil {
					log.WithError(err).Warn("db scan failed")
					continue
				}
				if slot != nil {
					s := uint64(*slot)
					ev.Slot = &s
				}
				if tokAddr != nil || tokSym != nil || tokDec != nil {
					ev.Token = &Token{Address: getOrEmpty(tokAddr), Symbol: getOrEmpty(tokSym)}
					if tokDec != nil {
						ev.Token.Decimals = uint8(*tokDec)
					}
				}
				out = append(out, &ev)
			}
			return out
		}
	}

	// Fallback in-memory
	s.mu.RLock()
	defer s.mu.RUnlock()
	filteredEvents := make([]*Event, len(s.events))
	copy(filteredEvents, s.events)
	if filter.Offset >= len(filteredEvents) {
		return []*Event{}
	}
	end := filter.Offset + filter.Limit
	if end > len(filteredEvents) {
		end = len(filteredEvents)
	}
	return filteredEvents[filter.Offset:end]
}

func NewHub() *Hub {
	return &Hub{
		clients:    make(map[chan []byte]struct{}),
		register:   make(chan chan []byte),
		unregister: make(chan chan []byte),
		broadcast:  make(chan []byte),
	}
}

func (h *Hub) Run() {
	for {
		select {
		case client := <-h.register:
			h.mu.Lock()
			h.clients[client] = struct{}{}
			h.mu.Unlock()
			log.Info("client registered")
		case client := <-h.unregister:
			h.mu.Lock()
			if _, ok := h.clients[client]; ok {
				delete(h.clients, client)
				close(client)
				log.Info("client unregistered")
			}
			h.mu.Unlock()
		case message := <-h.broadcast:
			h.mu.Lock()
			for client := range h.clients {
				select {
				case client <- message:
				default:
					close(client)
					delete(h.clients, client)
				}
			}
			h.mu.Unlock()
		}
	}
}

func healthHandler(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	_ = json.NewEncoder(w).Encode(Health{Status: "OK"})
}

func subscribeToEvents(ctx context.Context, redisURL string, store *EventStore, hub *Hub) {
	opt, err := redis.ParseURL(redisURL)
	if err != nil {
		log.Fatalf("could not parse redis url: %v", err)
	}

	rdb := redis.NewClient(opt)
	pubsub := rdb.Subscribe(ctx, "cross_chain_events")
	defer pubsub.Close()

	ch := pubsub.Channel()

	log.Info("subscribing to cross_chain_events")

	for msg := range ch {
		var event Event
		if err := json.Unmarshal([]byte(msg.Payload), &event); err != nil {
			log.WithError(err).Error("could not unmarshal event")
			continue
		}
		log.Infof("received event: %+v", event)

		// Attempt to persist to DB first (idempotent on event_id)
		if store.db != nil {
			if err := persistEvent(ctx, store.db, &event); err != nil {
				log.WithError(err).Warn("failed to persist event to db")
			}
		}

		// Always add to in-memory cache for SSE and fast reads
		store.Add(&event)
		hub.broadcast <- []byte(msg.Payload)
	}
}

func serveSSE(hub *Hub, w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("Connection", "keep-alive")
	w.Header().Set("Access-Control-Allow-Origin", "*")

	messageChan := make(chan []byte)
	hub.register <- messageChan
	defer func() {
		hub.unregister <- messageChan
	}()

	notify := r.Context().Done()
	go func() {
		<-notify
		hub.unregister <- messageChan
	}()

	for {
		select {
		case message, ok := <-messageChan:
			if !ok {
				return
			}
			fmt.Fprintf(w, "data: %s\n\n", message)
			if f, ok := w.(http.Flusher); ok {
				f.Flush()
			}
		case <-time.After(30 * time.Second): // Keep-alive
			fmt.Fprintf(w, ": keep-alive\n\n")
			if f, ok := w.(http.Flusher); ok {
				f.Flush()
			}
		}
	}
}

func getWalletTransactions(store *EventStore, w http.ResponseWriter, r *http.Request) {
	address := strings.ToLower(chi.URLParam(r, "address"))

	filter := EventFilter{
		Limit:  50,
		Offset: 0,
	}

	if limitStr := r.URL.Query().Get("limit"); limitStr != "" {
		if limit, err := strconv.Atoi(limitStr); err == nil {
			filter.Limit = limit
		}
	}
	if offsetStr := r.URL.Query().Get("offset"); offsetStr != "" {
		if offset, err := strconv.Atoi(offsetStr); err == nil {
			filter.Offset = offset
		}
	}

	filter.Chain = r.URL.Query().Get("chain")
	filter.Token = r.URL.Query().Get("token")
	filter.From = r.URL.Query().Get("from")
	filter.To = r.URL.Query().Get("to")

	if minValueStr := r.URL.Query().Get("min_value"); minValueStr != "" {
		if minValue, err := strconv.ParseFloat(minValueStr, 64); err == nil {
			filter.MinValue = minValue
		}
	}

	if startTimeStr := r.URL.Query().Get("start_time"); startTimeStr != "" {
		if startTime, err := time.Parse(time.RFC3339, startTimeStr); err == nil {
			filter.StartTime = &startTime
		}
	}

	if endTimeStr := r.URL.Query().Get("end_time"); endTimeStr != "" {
		if endTime, err := time.Parse(time.RFC3339, endTimeStr); err == nil {
			filter.EndTime = &endTime
		}
	}

	events := store.GetByWallet(address, filter)
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(events)
}

func getTransactions(store *EventStore, w http.ResponseWriter, r *http.Request) {
	filter := EventFilter{
		Limit:  50,
		Offset: 0,
	}

	if limitStr := r.URL.Query().Get("limit"); limitStr != "" {
		if limit, err := strconv.Atoi(limitStr); err == nil {
			filter.Limit = limit
		}
	}
	if offsetStr := r.URL.Query().Get("offset"); offsetStr != "" {
		if offset, err := strconv.Atoi(offsetStr); err == nil {
			filter.Offset = offset
		}
	}

	filter.Chain = r.URL.Query().Get("chain")
	filter.Token = r.URL.Query().Get("token")
	filter.From = r.URL.Query().Get("from")
	filter.To = r.URL.Query().Get("to")

	if minValueStr := r.URL.Query().Get("min_value"); minValueStr != "" {
		if minValue, err := strconv.ParseFloat(minValueStr, 64); err == nil {
			filter.MinValue = minValue
		}
	}

	if startTimeStr := r.URL.Query().Get("start_time"); startTimeStr != "" {
		if startTime, err := time.Parse(time.RFC3339, startTimeStr); err == nil {
			filter.StartTime = &startTime
		}
	}

	if endTimeStr := r.URL.Query().Get("end_time"); endTimeStr != "" {
		if endTime, err := time.Parse(time.RFC3339, endTimeStr); err == nil {
			filter.EndTime = &endTime
		}
	}

	filter.SortBy = r.URL.Query().Get("sort_by")
	filter.SortOrder = r.URL.Query().Get("sort_order")

	events := store.GetRecent(filter)
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(events)
}

func main() {
	log.SetFormatter(&log.JSONFormatter{})
	log.Info("starting api server")

	redisURL := os.Getenv("REDIS_URL")
	if redisURL == "" {
		log.Fatalf("REDIS_URL must be set")
	}

	store := NewEventStore(maxEvents, maxEventsPerWallet)
	// Optional Postgres backing for persistence
	if dsn := os.Getenv("POSTGRES_DSN"); dsn != "" {
		db, err := pgxpool.New(context.Background(), dsn)
		if err != nil {
			log.WithError(err).Warn("failed to connect to postgres; running in memory-only mode")
		} else {
			if err := initDB(context.Background(), db); err != nil {
				log.WithError(err).Warn("failed to init db schema; running in memory-only mode")
			} else {
				store.AttachDB(db)
				log.Info("api: connected to postgres and initialized schema")
			}
		}
	}
	hub := NewHub()
	go hub.Run()

	go subscribeToEvents(context.Background(), redisURL, store, hub)

	r := chi.NewRouter()
	r.Get("/health", healthHandler)
	r.Get("/events/subscribe", func(w http.ResponseWriter, r *http.Request) {
		serveSSE(hub, w, r)
	})
	r.Get("/wallet/{address}/transactions", func(w http.ResponseWriter, r *http.Request) {
		getWalletTransactions(store, w, r)
	})
	r.Get("/transactions", func(w http.ResponseWriter, r *http.Request) {
		getTransactions(store, w, r)
	})

	// Test endpoint - only enabled in test mode
	if os.Getenv("TEST_MODE") == "true" {
		r.Get("/internal/last-received", func(w http.ResponseWriter, r *http.Request) {
			limit := 1
			if limitStr := r.URL.Query().Get("limit"); limitStr != "" {
				if l, err := strconv.Atoi(limitStr); err == nil && l > 0 {
					limit = l
				}
			}

			filter := EventFilter{
				Limit:  limit,
				Offset: 0,
			}
			events := store.GetRecent(filter)
			w.Header().Set("Content-Type", "application/json")
			_ = json.NewEncoder(w).Encode(events)
		})
	}

	bindAddr := os.Getenv("BIND_ADDR")
	if bindAddr == "" {
		bindAddr = "0.0.0.0:8080"
	}

	// Create HTTP server with timeouts to prevent resource exhaustion (fixes G114)
	server := &http.Server{
		Addr:              bindAddr,
		Handler:           r,
		ReadTimeout:       15 * time.Second,
		ReadHeaderTimeout: 10 * time.Second,
		WriteTimeout:      15 * time.Second,
		IdleTimeout:       60 * time.Second,
		MaxHeaderBytes:    1 << 20, // 1 MB
	}

	log.Infof("api: listening on %s", bindAddr)
	log.Fatalf("server failed to start: %v", server.ListenAndServe())
}

// --- DB helpers ---

func initDB(ctx context.Context, db *pgxpool.Pool) error {
	_, err := db.Exec(ctx, `
		CREATE TABLE IF NOT EXISTS events (
			event_id TEXT PRIMARY KEY,
			chain TEXT NOT NULL,
			network TEXT NOT NULL,
			tx_hash TEXT NOT NULL,
			timestamp TEXT NOT NULL,
			from_addr TEXT NOT NULL,
			to_addr TEXT NOT NULL,
			value TEXT NOT NULL,
			event_type TEXT NOT NULL,
			slot BIGINT NULL,
			token_address TEXT NULL,
			token_symbol TEXT NULL,
			token_decimals INT NULL,
			created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
		);
		CREATE INDEX IF NOT EXISTS idx_events_from ON events (LOWER(from_addr));
		CREATE INDEX IF NOT EXISTS idx_events_to ON events (LOWER(to_addr));
		CREATE INDEX IF NOT EXISTS idx_events_created ON events (created_at DESC);
	`)
	return err
}

func persistEvent(ctx context.Context, db *pgxpool.Pool, ev *Event) error {
	var slot *int64
	if ev.Slot != nil {
		tmp := int64(*ev.Slot)
		slot = &tmp
	}
	var tokAddr, tokSym *string
	var tokDec *int32
	if ev.Token != nil {
		ta := ev.Token.Address
		ts := ev.Token.Symbol
		td := int32(ev.Token.Decimals)
		tokAddr = &ta
		tokSym = &ts
		tokDec = &td
	}
	_, err := db.Exec(ctx, `
		INSERT INTO events (event_id, chain, network, tx_hash, timestamp, from_addr, to_addr, value, event_type, slot, token_address, token_symbol, token_decimals)
		VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
		ON CONFLICT (event_id) DO NOTHING
	`,
		ev.EventID, ev.Chain, ev.Network, ev.TxHash, ev.Timestamp,
		ev.From, ev.To, ev.Value, ev.EventType, slot, tokAddr, tokSym, tokDec,
	)
	return err
}

func getOrEmpty(s *string) string {
	if s == nil {
		return ""
	}
	return *s
}
