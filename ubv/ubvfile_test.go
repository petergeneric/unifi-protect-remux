package ubv

import (
	"log"
	"testing"
	"time"
)

func TestGenerateTimecode(t *testing.T) {
	timecode := GenerateTimecode(time.Date(2023, time.Month(5), 16, 11, 58, 26, 500000000, time.UTC), 30)
	log.Printf("Timecode Generated")
	if timecode != "11:58:26.16" {
		t.Errorf("Timecode generated is incorrect, got: %s, want: %s.", timecode, "11:58:26.16")
	}
}
