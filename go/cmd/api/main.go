package main

import (
	"encoding/json"
	"log"
	"net/http"
)

type Health struct {
	Status string `json:"status"`
}

func healthHandler(w http.ResponseWriter, r *http.Request) {
	w.WriteHeader(http.StatusOK)
	json.NewEncoder(w).Encode(Health{Status: "OK"})
}

func main() {
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
