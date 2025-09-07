#!/usr/bin/env python3

def find_bssid_end(text):
    """Python version of the Rust find_bssid_end function"""
    escaped_colons = 0
    
    for i, ch in enumerate(text):
        if ch == ':' and i > 0 and text[i-1] == '\\':
            escaped_colons += 1
        elif ch == ':' and (i == 0 or text[i-1] != '\\'):
            # Found unescaped colon - this should be end of BSSID
            if escaped_colons == 5 and i >= 17:
                # Valid MAC address has 5 escaped colons
                return i
            else:
                return i  # Best guess at BSSID end
    
    # If no unescaped colon found, return None
    return None

def parse_nmcli_line(line):
    """Test parsing of nmcli output"""
    print(f"Parsing: {line}")
    
    # Find first colon (end of SSID)
    ssid_end = line.find(':')
    if ssid_end == -1:
        print("  ❌ No colon found")
        return None
    
    ssid = line[:ssid_end].strip()
    rest = line[ssid_end + 1:]
    
    print(f"  SSID: '{ssid}'")
    print(f"  Rest: '{rest}'")
    
    # Find BSSID end
    bssid_end = find_bssid_end(rest)
    if bssid_end is None:
        print("  ❌ No BSSID end found")
        return None
    
    bssid_raw = rest[:bssid_end].strip()
    remaining = rest[bssid_end + 1:]
    
    print(f"  BSSID raw: '{bssid_raw}'")
    print(f"  Remaining: '{remaining}'")
    
    # Parse remaining as signal:freq
    parts = remaining.split(':')
    if len(parts) >= 2:
        signal_str = parts[0].strip()
        freq_str = parts[1].strip()
        
        bssid = bssid_raw.replace('\\:', ':')
        
        print(f"  BSSID clean: '{bssid}'")
        print(f"  Signal: '{signal_str}'")
        print(f"  Frequency: '{freq_str}'")
        
        return {
            'ssid': ssid,
            'bssid': bssid,
            'signal': signal_str,
            'frequency': freq_str
        }
    else:
        print("  ❌ Not enough remaining parts")
        return None

# Test with actual nmcli output
test_lines = [
    "trucmuche (5Ghz):74\\:4D\\:28\\:5F\\:F0\\:49:94:5500 MHz",
    "trucmuche (2Ghz):00\\:01\\:02\\:00\\:03\\:FF:84:2417 MHz",
    "FreeWifi_secure:00\\:01\\:02\\:00\\:05\\:00:79:2417 MHz"
]

for line in test_lines:
    result = parse_nmcli_line(line)
    print(f"  ✅ Result: {result}\n" if result else "  ❌ Failed\n")