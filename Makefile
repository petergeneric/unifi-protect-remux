GIT_COMMIT=$(shell git rev-list -1 HEAD)
GIT_TAG=$(shell git describe --tags)


all: package

clean:
	-rm -f remux *.h264 *.aac *.mp4

package: clean
	go build -ldflags "-X main.GitCommit=${GIT_COMMIT} -X main.ReleaseVersion=${GIT_TAG}" *.go
