# 🤖 Machine Learning Features in network-dmenu

This guide explains how to enable and use the machine learning features in network-dmenu to get intelligent network management capabilities.

## 📋 Overview

The ML features in network-dmenu provide:
- **Smart Exit Node Selection** - Predicts best Tailscale/Mullvad exit nodes based on performance history
- **Personalized Menu Ordering** - Learns your usage patterns to prioritize frequently used actions
- **Network Diagnostics** - Analyzes network issues and suggests fixes
- **WiFi Network Prediction** - Recommends best WiFi networks to connect to
- **Performance Tracking** - Monitors and learns from network performance over time

## 🚀 Quick Start

### 1. Build with ML Features Enabled

```bash
# Clone the repository if you haven't already
git clone https://github.com/cyrinux/network-dmenu
cd network-dmenu

# Build with ML features enabled
cargo build --release --features ml

# Install the binary (optional)
sudo cp target/release/network-dmenu /usr/local/bin/
```

### 2. First Run

The ML features work automatically once enabled. Just run network-dmenu as usual:

```bash
network-dmenu
```

The ML system will:
- Start learning from your selections immediately
- Build performance profiles for exit nodes as you use them
- Personalize menu ordering based on your usage patterns
- Save learned models to `~/.local/share/network-dmenu/ml/`

## 🎯 Features in Detail

### Smart Exit Node Selection

When you have ML enabled, Tailscale exit nodes are automatically sorted by predicted performance:

1. **Automatic Scoring** - Each exit node gets a score based on:
   - Geographic distance (estimated)
   - Historical latency and stability
   - Current load factor
   - Time of day patterns
   - Your past usage patterns

2. **Visual Indicators** - Nodes with high ML confidence appear first in the list

3. **Continuous Learning** - The system records performance after each connection to improve predictions

### Personalized Menu Ordering

The menu learns your habits and reorganizes itself:

1. **Time-Based Patterns** - Different ordering for morning vs evening
2. **Day-Based Patterns** - Workday vs weekend preferences
3. **Location Awareness** - Different preferences at home vs office
4. **Frequency Tracking** - Most used actions bubble to the top

### Network Diagnostics

When experiencing network issues:

```bash
# The ML system will analyze symptoms and suggest tests
network-dmenu --diagnostics
```

The diagnostic analyzer can:
- Identify root causes from symptoms
- Recommend specific diagnostic tests
- Learn from past issues and resolutions

### WiFi Network Prediction

The ML system ranks WiFi networks by:
- Signal strength and quality
- Historical performance
- Time-of-day patterns
- Security level preferences
- Past connection success rates

## 📊 Monitoring ML Performance

### View ML Statistics

Check how well the ML system is performing:

```bash
# View performance summary for a specific connection
network-dmenu --ml-stats "exit-node-name"
```

### Model Information

ML models are stored in `~/.local/share/network-dmenu/ml/`:
- `exit_node.json` - Exit node predictor model
- `diagnostic.json` - Diagnostic analyzer model
- `network.json` - WiFi network predictor
- `performance.json` - Performance tracking data
- `usage.json` - Usage pattern learner

### Reset ML Models

To start fresh with ML learning:

```bash
rm -rf ~/.local/share/network-dmenu/ml/
```

## ⚙️ Configuration

### ML-Specific Config Options

Add to your `~/.config/network-dmenu/config.toml`:

```toml
[ml]
# Minimum confidence threshold for ML predictions (0.0 to 1.0)
confidence_threshold = 0.75

# Path where ML models are stored
model_path = "~/.local/share/network-dmenu/ml"

# Enable specific ML features
enable_exit_node_prediction = true
enable_menu_personalization = true
enable_diagnostic_analysis = true
enable_wifi_prediction = true

# Training parameters
min_samples_for_training = 50
retrain_interval_hours = 24
```

### Disable ML Features Temporarily

Run without ML features even when compiled with them:

```bash
# Set environment variable
NETWORK_DMENU_NO_ML=1 network-dmenu
```

## 🔍 How It Works

### Data Collection

The ML system collects:
- **Performance Metrics** - Latency, packet loss, bandwidth
- **Usage Patterns** - What actions you select and when
- **Network Context** - Time of day, network type, location hash
- **Success/Failure** - Whether actions completed successfully

### Privacy

- **All data stays local** - No data is sent to external servers
- **Hashed locations** - Location data is hashed, not stored in plain text
- **No personal info** - Only technical metrics and patterns are recorded
- **User control** - You can delete ML data at any time

### Learning Process

1. **Initial Phase** (0-50 actions)
   - System operates normally without ML predictions
   - Collects baseline data

2. **Learning Phase** (50-500 actions)
   - Begins making predictions with lower confidence
   - Continuously adjusts based on feedback

3. **Mature Phase** (500+ actions)
   - High-confidence predictions
   - Refined personalization
   - Stable performance profiles

## 🐛 Troubleshooting

### ML Features Not Working

1. **Verify ML build**:
   ```bash
   network-dmenu --version
   # Should show features including 'ml'
   ```

2. **Check model files exist**:
   ```bash
   ls ~/.local/share/network-dmenu/ml/
   ```

3. **Enable debug logging**:
   ```bash
   RUST_LOG=debug network-dmenu
   ```

### Poor Predictions

- Let the system collect more data (at least 50-100 actions)
- Check if patterns are consistent (ML can't predict random behavior)
- Reset models if they've learned incorrect patterns

### Performance Impact

ML features add minimal overhead:
- Model inference: <10ms
- Memory usage: ~5-10MB for models
- Disk usage: <1MB for saved models

To disable ML temporarily if experiencing issues:
```bash
network-dmenu --no-ml
```

## 📈 Best Practices

1. **Use Consistently** - The more you use it, the better it learns
2. **Complete Actions** - Let connections fully establish for accurate performance data
3. **Vary Usage** - Use different features to train all models
4. **Regular Use** - Models improve with regular, varied usage patterns

## 🔮 Future ML Enhancements

Planned improvements:
- Predictive network failure detection
- Automatic failover suggestions
- Battery-aware optimizations
- Bandwidth prediction and optimization
- Multi-device learning synchronization

## 📝 Examples

### Example 1: Smart Exit Node Selection

When you select "Tailscale Exit Nodes", the ML system:
1. Analyzes current context (time, location, network type)
2. Recalls performance history for each available node
3. Predicts performance scores
4. Sorts nodes with best predicted performance first
5. Records actual performance after connection

### Example 2: Personalized Workflow

Monday morning at office:
- VPN connections appear first
- Work WiFi network prioritized
- Specific exit nodes for work resources highlighted

Friday evening at home:
- Entertainment-related connections prioritized
- Home WiFi network at top
- Gaming-optimized exit nodes suggested

### Example 3: Diagnostic Intelligence

Network slow? The ML system might suggest:
```
Detected: High latency pattern
Likely cause: DNS issues (confidence: 85%)
Recommended tests:
1. Test DNS resolution
2. Check DNS cache
3. Try alternative DNS servers
```

## 🤝 Contributing

Help improve ML features:
1. Report prediction accuracy issues
2. Suggest new ML use cases
3. Contribute training data patterns
4. Help optimize model performance

## 📚 Technical Details

### Models Used
- **Random Forest** for exit node prediction
- **Pattern matching** for diagnostic analysis
- **Time-series analysis** for performance tracking
- **Clustering** for usage pattern learning

### Feature Engineering
- Temporal features (time of day, day of week)
- Network features (type, signal strength)
- Historical features (past performance metrics)
- Context features (location hash, active applications)

### Training Pipeline
1. Data collection and preprocessing
2. Feature extraction and normalization
3. Model training with cross-validation
4. Model evaluation and selection
5. Deployment and continuous learning

---

*The ML features are designed to be helpful but not intrusive. They enhance your experience while maintaining the simplicity and speed of network-dmenu.*