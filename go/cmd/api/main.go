package main

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"strconv"
	"sync"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/go-redis/redis/v8"
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
}

func NewEventStore(maxTotalEvents, maxEventsPerWallet int) *EventStore {
	return &EventStore{
		events:             make([]*Event, 0),
		eventsByWallet:     make(map[string][]*Event),
		maxTotalEvents:     maxTotalEvents,
		maxEventsPerWallet: maxEventsPerWallet,
	}
}

func (s *EventStore) Add(event *Event) {
	s.mu.Lock()
	defer s.mu.Unlock()

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
	s.mu.RLock()
	defer s.mu.RUnlock()

	var filteredEvents []*Event

	// Apply filters
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
			val, err := strconv.ParseFloat(event.Value, 64)
			if err != nil || val < filter.MinValue {
				continue
			}
		}
		if filter.StartTime != nil || filter.EndTime != nil {
			eventTime, err := time.Parse(time.RFC3339, event.Timestamp)
			if err != nil {
				log.WithError(err).Warn("could not parse event timestamp")
				continue
			}
			if filter.StartTime != nil && eventTime.Before(*filter.StartTime) {
				continue
			}
			if filter.EndTime != nil && eventTime.After(*filter.EndTime) {
				continue
			}
		}
		filteredEvents = append(filteredEvents, event)
	}

	// Pagination
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
	s.mu.RLock()
	defer s.mu.RUnlock()

	var filteredEvents []*Event

	// Apply filters
	for _, event := range s.events {
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
			val, err := strconv.ParseFloat(event.Value, 64)
			if err != nil || val < filter.MinValue {
				log.WithError(err).Warn("could not parse event value")
				continue
			}
		}
		if filter.StartTime != nil || filter.EndTime != nil {
			eventTime, err := time.Parse(time.RFC3339, event.Timestamp)
			if err != nil {
				log.WithError(err).Warn("could not parse event timestamp")
				continue
			}
			if filter.StartTime != nil && eventTime.Before(*filter.StartTime) {
				continue
			}
			if filter.EndTime != nil && eventTime.After(*filter.EndTime) {
				continue
			}
		}
		filteredEvents = append(filteredEvents, event)
	}

	// Sorting
	if filter.SortBy != "" {
		// For simplicity, let's assume sorting by Timestamp for now
		// A more robust solution would involve a sort.Slice stable sort
		// and handling different sortable fields.
		// Since events are added in reverse chronological order, GetRecent naturally returns
		// events sorted by timestamp descending. If ascending is requested, we reverse.
		if filter.SortBy == "timestamp" && filter.SortOrder == "asc" {
			for i, j := 0, len(filteredEvents)-1; i < j; i, j = i+1, j-1 {
				filteredEvents[i], filteredEvents[j] = filteredEvents[j], filteredEvents[i]
			}
		}
	}

	// Pagination
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
	json.NewEncoder(w).Encode(Health{Status: "OK"})
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
	address := chi.URLParam(r, "address")

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
	json.NewEncoder(w).Encode(events)
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
	json.NewEncoder(w).Encode(events)
}

func main() {
	log.SetFormatter(&log.JSONFormatter{})
	log.Info("starting api server")

	redisURL := os.Getenv("REDIS_URL")
	if redisURL == "" {
		log.Fatalf("REDIS_URL must be set")
	}

	store := NewEventStore(maxEvents, maxEventsPerWallet)
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

	log.Info("api: listening on :8080")
	log.Fatalf("server failed to start: %v", http.ListenAndServe("127.0.0.1:8080", r))
}
