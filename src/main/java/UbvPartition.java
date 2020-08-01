import java.time.Instant;
import java.util.ArrayList;
import java.util.List;

public class UbvPartition
{
	public final int index;
	public Instant firstFrameTimecode;
	public final List<FrameDataRef> frames = new ArrayList<>();


	public UbvPartition(final int index)
	{
		this.index = index;
	}
}
