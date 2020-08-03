all: package

clean:
	mvn clean

package: clean
	mvn package
	
# Uses GraalVM's native-image tool to produce a native binary
native-image: package
	native-image --no-fallback -cp target/*.jar com.peterphi.unifiprotect.remux.Remux
