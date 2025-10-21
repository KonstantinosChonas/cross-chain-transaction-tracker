package main

import (
	"context"
	"encoding/json"
	"log"
	"net/http"
	"os"

	"github.com/go-redis/redis/v8"
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

func healthHandler(w http.ResponseWriter, r *http.Request) {
	w.WriteHeader(http.StatusOK)
	json.NewEncoder(w).Encode(Health{Status: "OK"})
}

func subscribeToEvents(ctx context.Context, redisURL string) {
	opt, err := redis.ParseURL(redisURL)
	if err != nil {
		log.Fatalf("could not parse redis url: %v", err)
	}

	rdb := redis.NewClient(opt)
	pubsub := rdb.Subscribe(ctx, "cross_chain_events")
	defer pubsub.Close()

	ch := pubsub.Channel()

	log.Println("subscribing to cross_chain_events")

	for msg := range ch {
		var event Event
		if err := json.Unmarshal([]byte(msg.Payload), &event); err != nil {
			log.Printf("could not unmarshal event: %v", err)
			continue
		}
		log.Printf("received event: %+v\n", event)
	}
}

func main() {
	redisURL := os.Getenv("REDIS_URL")
	if redisURL == "" {
		log.Fatal("REDIS_URL must be set")
	}

	go subscribeToEvents(context.Background(), redisURL)

	http.HandleFunc("/health", healthHandler)
	// Simple endpoint for events (for Phase A)
	http.HandleFunc("/events/ingest", func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			w.WriteHeader(http.StatusMethodNotAllowed)
			return
		}
		var raw map[string]interface{}
		if err := json.NewDecoder(r.Body).Decode(&raw); err != nil {
			http.Error(w, "bad payload", http.StatusBadRequest)
			return
		}
		log.Printf("ingested event: %+v\n", raw)
		w.WriteHeader(http.StatusAccepted)
	})

	log.Println("api: listening on :8080")
	log.Fatal(http.ListenAndServe("127.0.0.1:8080", nil))
}
