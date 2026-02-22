MAGICK := magick
SVG := ubv-document.svg
ICO := ubv-document.ico
SIZES := 16 20 24 32 40 48 64 256

PNGS := $(foreach s,$(SIZES),ubv-document-$(s).png)

.PHONY: all clean

all: $(ICO)

$(ICO): $(PNGS)
	$(MAGICK) $(PNGS) $(ICO)

ubv-document-%.png: $(SVG)
	$(MAGICK) -background none -density 384 $< -resize $*x$* $@

clean:
	rm -f $(PNGS) $(ICO)
