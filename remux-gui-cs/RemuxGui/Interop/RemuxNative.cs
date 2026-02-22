using System;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text.Json;
using RemuxGui.Models;

namespace RemuxGui.Interop;

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
public delegate void ProgressCallback(IntPtr jsonEvent, int fileIndex);

public static class RemuxNative
{
    private const string LibName = "remux_ffi";

    static RemuxNative()
    {
        NativeLibrary.SetDllImportResolver(typeof(RemuxNative).Assembly, ResolveLibrary);
    }

    private static IntPtr ResolveLibrary(string libraryName, Assembly assembly, DllImportSearchPath? searchPath)
    {
        if (libraryName != LibName)
            return IntPtr.Zero;

        string? nativePath = null;

        if (RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
        {
            nativePath = System.IO.Path.Combine(AppContext.BaseDirectory, "remux_ffi.dll");
        }
        else if (RuntimeInformation.IsOSPlatform(OSPlatform.OSX))
        {
            nativePath = System.IO.Path.Combine(AppContext.BaseDirectory, "libremux_ffi.dylib");
        }
        else if (RuntimeInformation.IsOSPlatform(OSPlatform.Linux))
        {
            nativePath = System.IO.Path.Combine(AppContext.BaseDirectory, "libremux_ffi.so");
        }

        if (nativePath != null && NativeLibrary.TryLoad(nativePath, out var handle))
            return handle;

        // Fall back to default resolution
        if (NativeLibrary.TryLoad(libraryName, assembly, searchPath, out handle))
            return handle;

        return IntPtr.Zero;
    }

    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    private static extern void remux_init();

    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr remux_version();

    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr remux_validate_config(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string configJson);

    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr remux_process_file(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string ubvPath,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string configJson,
        ProgressCallback? progressCallback,
        int fileIndex,
        out IntPtr errorOut);

    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr remux_produce_diagnostics(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string ubvPath,
        out IntPtr errorOut);

    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
    private static extern void remux_free_string(IntPtr s);

    /// <summary>
    /// Read a UTF-8 string from a Rust-allocated pointer, then free it.
    /// Returns null if the pointer is IntPtr.Zero.
    /// </summary>
    private static string? ReadAndFreeString(IntPtr ptr)
    {
        if (ptr == IntPtr.Zero)
            return null;
        try
        {
            return Marshal.PtrToStringUTF8(ptr);
        }
        finally
        {
            remux_free_string(ptr);
        }
    }

    /// <summary>
    /// Read a UTF-8 string from a Rust-allocated error pointer, then free it.
    /// Returns null if the pointer is IntPtr.Zero.
    /// </summary>
    private static string? ReadAndFreeError(IntPtr errorPtr)
    {
        if (errorPtr == IntPtr.Zero)
            return null;
        try
        {
            return Marshal.PtrToStringUTF8(errorPtr);
        }
        finally
        {
            remux_free_string(errorPtr);
        }
    }

    // --- Public API ---

    public static void Init()
    {
        remux_init();
    }

    public static VersionInfo GetVersion()
    {
        var json = ReadAndFreeString(remux_version());
        if (json == null)
            return new VersionInfo { Version = "unknown" };
        return JsonSerializer.Deserialize<VersionInfo>(json) ?? new VersionInfo { Version = "unknown" };
    }

    public static (bool valid, string? error) ValidateConfig(RemuxConfig config)
    {
        var configJson = JsonSerializer.Serialize(config);
        var resultJson = ReadAndFreeString(remux_validate_config(configJson));
        if (resultJson == null)
            return (false, "Internal error: null result from validate_config");

        using var doc = JsonDocument.Parse(resultJson);
        var valid = doc.RootElement.GetProperty("valid").GetBoolean();
        string? error = null;
        if (doc.RootElement.TryGetProperty("error", out var errorProp))
            error = errorProp.GetString();
        return (valid, error);
    }

    public static (string? resultJson, string? error) ProcessFile(
        string ubvPath,
        RemuxConfig config,
        ProgressCallback? callback,
        int fileIndex)
    {
        var configJson = JsonSerializer.Serialize(config);
        var resultPtr = remux_process_file(ubvPath, configJson, callback, fileIndex, out var errorPtr);
        var error = ReadAndFreeError(errorPtr);
        var result = ReadAndFreeString(resultPtr);
        return (result, error);
    }

    public static (string? outputPath, string? error) ProduceDiagnostics(string ubvPath)
    {
        var resultPtr = remux_produce_diagnostics(ubvPath, out var errorPtr);
        var error = ReadAndFreeError(errorPtr);
        var resultJson = ReadAndFreeString(resultPtr);

        if (resultJson != null)
        {
            using var doc = JsonDocument.Parse(resultJson);
            var outputPath = doc.RootElement.GetProperty("output_path").GetString();
            return (outputPath, null);
        }
        return (null, error ?? "Unknown error");
    }
}
