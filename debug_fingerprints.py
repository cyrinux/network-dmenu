#!/usr/bin/env python3
import json
import subprocess
from datetime import datetime

def get_current_wifi():
    """Get current WiFi networks using nmcli"""
    try:
        result = subprocess.run(['nmcli', '--colors', 'no', '-t', '-f', 'SSID,BSSID,SIGNAL,FREQ', 'device', 'wifi'], 
                              capture_output=True, text=True)
        if result.returncode == 0:
            networks = []
            for line in result.stdout.strip().split('\n'):
                if line.strip():
                    parts = line.split(':')
                    if len(parts) >= 4:
                        ssid = parts[0]
                        bssid = parts[1].replace('\\:', ':')
                        try:
                            signal = int(parts[2])
                            freq = int(parts[3].replace(' MHz', '').strip())
                            networks.append({
                                'ssid': ssid,
                                'bssid': bssid,
                                'signal': signal,
                                'freq': freq
                            })
                        except:
                            continue
            return networks
    except Exception as e:
        print(f"Error getting WiFi networks: {e}")
    return []

def load_zone_data():
    """Load zone data from JSON"""
    with open('/home/cyril/.local/share/network-dmenu/zones.json', 'r') as f:
        return json.load(f)

def analyze_similarity():
    """Analyze similarity between current location and home zone"""
    current_networks = get_current_wifi()
    zones = load_zone_data()
    
    home_zone = None
    for zone in zones:
        if zone['name'] == 'home':
            home_zone = zone
            break
    
    if not home_zone:
        print("Home zone not found!")
        return
    
    print(f"Current location: {len(current_networks)} WiFi networks detected")
    print(f"Home zone has {len(home_zone['fingerprints'])} fingerprints")
    
    for i, fingerprint in enumerate(home_zone['fingerprints']):
        print(f"\n--- Fingerprint {i+1} ---")
        print(f"Zone networks: {len(fingerprint['wifi_networks'])}")
        print(f"Zone confidence: {fingerprint['confidence_score']}")
        
        # Compare network names (SSIDs) by hash
        current_ssids = set()
        for net in current_networks:
            if net['ssid']:
                # Create a hash-like representation
                current_ssids.add(net['ssid'][:8] if len(net['ssid']) > 8 else net['ssid'])
        
        zone_ssids = set()
        for net in fingerprint['wifi_networks']:
            zone_ssids.add(net['ssid_hash'][:8])
        
        print(f"Current networks (first 8 chars): {sorted(current_ssids)}")
        print(f"Zone networks (first 8 chars): {sorted(zone_ssids)}")
        
        # Simple intersection calculation
        intersection = len(current_ssids.intersection(zone_ssids))
        union = len(current_ssids.union(zone_ssids))
        jaccard = intersection / union if union > 0 else 0
        
        print(f"Intersection: {intersection}, Union: {union}, Jaccard: {jaccard:.3f}")
        
        if jaccard >= 0.8:
            print("✅ Would match (>= 0.8 threshold)")
        else:
            print("❌ Below threshold (< 0.8)")

if __name__ == "__main__":
    analyze_similarity()