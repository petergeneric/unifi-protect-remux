all: package

clean:
	-rm -f remux

package: clean
	go build *.go
