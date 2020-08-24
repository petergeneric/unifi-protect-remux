GIT_COMMIT=$(shell git rev-list -1 HEAD)
GIT_TAG=$(shell git describe --tags $(git rev-list --tags --max-count=1 2>/dev/null) 2>/dev/null)


all: package

clean:
	-rm -f remux *.h264 *.aac *.mp4

package: clean
	go build -ldflags "-X main.GitCommit=${GIT_COMMIT} -X main.ReleaseVersion=${GIT_TAG}" *.go
