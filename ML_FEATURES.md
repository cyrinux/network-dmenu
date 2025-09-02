# 🤖 Machine Learning Features for network-dmenu

## Overview

network-dmenu now includes optional machine learning capabilities that enhance network management through intelligent predictions, pattern recognition, and adaptive personalization. These features are designed to be lightweight, fast, and seamlessly integrated with the existing functional programming architecture.

## Features

### 1. 🎯 Intelligent Exit Node Selection

The ML-powered exit node predictor learns from historical performance data to recommend the best Tailscale/Mullvad exit nodes based on:

- **Historical Performance**: Latency, packet loss, and stability metrics
- **Geographic Optimization**: Distance and location-based routing
- **Time-based Patterns**: Peak hour performance predictions
- **Network Conditions**: Current network state and quality

#### How It Works
- Collects performance metrics for each exit node over time
- Uses Random Forest regression to predict node performance
- Considers factors like time of day, network type, and location
- Provides confidence scores for predictions

### 2. 🔍 Smart Network Diagnostics

The diagnostic analyzer uses pattern recognition to:

- **Identify Root Causes**: Maps symptoms to probable network issues
- **Recommend Tests**: Suggests specific diagnostic tests based on symptoms
- **Predict Failures**: Detects degrading performance before failures occur
- **Learn from History**: Improves accuracy over time

#### Symptom Recognition
- High latency → Network congestion
- DNS failures → DNS server issues
- Packet loss → Gateway problems
- Intermittent connection → WiFi interference

### 3. 🎨 Personalized Menu Ordering

The usage pattern learner adapts the menu to your habits:

- **Frequency-based Ranking**: Most-used items appear first
- **Context-aware Ordering**: Adapts based on time and location
- **Workflow Detection**: Recognizes common action sequences
- **Predictive Actions**: Suggests likely next actions

### 4. 📶 WiFi Network Quality Prediction

Predicts the best WiFi network to connect to based on:

- **Signal Strength Analysis**: Beyond simple RSSI values
- **Historical Performance**: Past connection success and quality
- **Security Preferences**: Prioritizes secure networks
- **Time-based Patterns**: Learns network availability patterns

### 5. 📊 Performance Tracking & Analysis

Continuous monitoring and analysis of network performance:

- **Real-time Metrics**: Latency, bandwidth, packet loss tracking
- **Trend Analysis**: Detects performance degradation
- **Alert Generation**: Notifies about performance issues
- **Summary Reports**: Comprehensive performance statistics

## Installation

### Building with ML Features

```bash
# Clone the repository
git clone https://github.com/cyrinux/network-dmenu.git
cd network-dmenu

# Build with ML features enabled
cargo build --release --features ml

# Or use the provided build script
./build_with_ml.sh
```

### Dependencies

The ML features require additional Rust crates:
- `smartcore`: Pure Rust machine learning algorithms
- `ndarray`: N-dimensional arrays for numerical computing
- `linfa`: Rust ML framework
- `linfa-trees`: Decision tree algorithms

These are automatically included when building with the `ml` feature flag.

## Usage

### Enabling ML Features

ML features are automatically enabled when built with the `ml` feature flag. The system will:

1. Create necessary directories at `~/.config/network-dmenu/ml/`
2. Initialize ML models on first run
3. Start collecting data for training
4. Begin making predictions once sufficient data is available

### Command Line Integration

All ML features integrate seamlessly with existing commands:

```bash
# Regular usage - ML will enhance selections automatically
network-dmenu

# ML will personalize menu ordering based on your usage
network-dmenu --no-diagnostics

# Exit node selection will use ML predictions when available
network-dmenu --country USA
```

### Configuration

Add ML-specific settings to your `~/.config/network-dmenu/config.toml`:

```toml
[ml]
enabled = true
model_path = "~/.config/network-dmenu/ml/models"
training_data_path = "~/.config/network-dmenu/ml/data"
min_training_samples = 100
update_frequency_hours = 24
confidence_threshold = 0.7

[ml.exit_node]
max_history_size = 1000
feature_window_size = 10
distance_weight = 0.2
latency_weight = 0.35
stability_weight = 0.25
priority_weight = 0.2

[ml.diagnostics]
min_confidence_threshold = 0.6
max_history_size = 500
pattern_match_threshold = 0.7

[ml.usage]
max_sequence_length = 5
max_history_size = 1000
workflow_threshold = 3
recency_weight = 0.4
frequency_weight = 0.35
context_weight = 0.25
```

## How ML Improves Your Experience

### Day 1: Learning Phase
- ML models initialize and start collecting data
- Traditional selection methods are used
- Every action and performance metric is recorded

### Week 1: Early Predictions
- Basic patterns emerge from your usage
- Menu items start reordering based on frequency
- Exit node recommendations begin appearing

### Month 1: Adaptive Intelligence
- Accurate predictions for network quality
- Personalized menu perfectly suited to your workflow
- Proactive problem detection and resolution
- Optimal exit node selection based on your patterns

## Privacy & Performance

### Privacy Considerations
- **All data stays local**: No cloud services or external APIs
- **No personal information**: Only network metrics and usage patterns
- **User control**: Can be disabled at any time
- **Data pruning**: Old data automatically removed

### Performance Impact
- **Minimal overhead**: < 10ms for predictions
- **Efficient storage**: < 10MB for typical usage
- **Background training**: Models update without blocking
- **Lazy loading**: ML components load only when needed

## Technical Details

### Algorithms Used

1. **Random Forest Regression**: Exit node performance prediction
2. **Pattern Matching**: Network diagnostic analysis
3. **Cosine Similarity**: Context and feature comparison
4. **Exponential Moving Average**: Time-series smoothing
5. **Bayesian Inference**: Confidence scoring

### Model Architecture

```
┌─────────────────┐
│   User Action   │
└────────┬────────┘
         │
    ┌────▼────┐
    │ Context │ (Time, Location, Network)
    └────┬────┘
         │
┌────────▼────────┐
│ Feature Extract │
└────────┬────────┘
         │
    ┌────▼────┐
    │ ML Model│
    └────┬────┘
         │
┌────────▼────────┐
│   Prediction    │
└────────┬────────┘
         │
    ┌────▼────┐
    │  Action │
    └─────────┘
```

### Data Flow

1. **Collection**: Network metrics and user actions recorded
2. **Processing**: Features extracted and normalized
3. **Training**: Models updated periodically (default: daily)
4. **Prediction**: Real-time inference on user requests
5. **Feedback**: Results used to improve future predictions

## Examples

### Example 1: ML-Enhanced Workflow

```bash
# Morning routine - ML knows you connect to work VPN
$ network-dmenu
> 🔒 Enable Tailscale VPN        # Appears first
> 🚀 Select Exit Node (us-nyc)   # Your usual choice
> 📊 Run Network Diagnostics     # Frequently used

# Evening - ML adapts to different context
$ network-dmenu
> 🎧 Connect Bluetooth Headphones # Evening pattern
> 📶 Connect to HomeWiFi-5G       # Preferred network
> 🌐 Disable Exit Node           # Common evening action
```

### Example 2: Intelligent Diagnostics

```bash
# Network issues detected
$ network-dmenu
> ⚠️ Network Issues Detected - Run Diagnostics?

# ML analyzes symptoms
Symptoms: High latency, Packet loss
Probable Cause: Network Congestion (85% confidence)
Recommended Tests:
  1. Measure Latency
  2. Traceroute to Gateway
  3. Speed Test
```

### Example 3: Exit Node Optimization

```bash
# ML predicts best exit nodes
$ network-dmenu
> 🌍 Exit Nodes (ML Enhanced)
  1. us-nyc-wg-301 ⭐ (Score: 92) - ML Recommended
  2. us-nyc-wg-302 (Score: 87)
  3. ca-tor-wg-201 (Score: 83)
  
# Automatic performance tracking
Selected: us-nyc-wg-301
Recording performance metrics...
Latency: 25ms, Loss: 0.1%
Model updated for future predictions
```

## Troubleshooting

### ML Features Not Working

1. **Check if ML is enabled in build**:
   ```bash
   cargo build --features ml
   ```

2. **Verify model directory exists**:
   ```bash
   ls ~/.config/network-dmenu/ml/
   ```

3. **Check logs for ML initialization**:
   ```bash
   RUST_LOG=debug network-dmenu 2>&1 | grep ML
   ```

### Poor Predictions

- **Insufficient data**: ML needs at least 100 samples
- **Stale models**: Delete `~/.config/network-dmenu/ml/models/` to retrain
- **Changed patterns**: Models adapt over time, be patient

### Performance Issues

- **Disable ML temporarily**:
  ```toml
  [ml]
  enabled = false
  ```

- **Reduce history size**:
  ```toml
  [ml.exit_node]
  max_history_size = 500  # Reduce from 1000
  ```

## Future Enhancements

### Planned Features
- 🔮 Predictive network switching
- 📈 Advanced time-series forecasting
- 🤝 Collaborative filtering (optional)
- 🎯 Multi-objective optimization
- 📱 Cross-device model sync
- 🔄 Online learning algorithms

### Research Areas
- Reinforcement learning for action sequences
- Federated learning for privacy-preserving improvements
- Neural architecture search for optimal models
- Transfer learning from similar network patterns

## Contributing

We welcome contributions to improve ML features! Areas of interest:

1. **New Algorithms**: Implement additional ML algorithms
2. **Feature Engineering**: Identify new predictive features
3. **Performance Optimization**: Reduce inference time
4. **Model Evaluation**: Improve testing and validation
5. **Documentation**: Enhance examples and guides

### Development Setup

```bash
# Clone and setup
git clone https://github.com/cyrinux/network-dmenu.git
cd network-dmenu

# Run tests with ML
cargo test --features ml

# Run ML example
cargo run --example ml_integration --features ml

# Benchmark ML performance
cargo bench --features ml
```

## License

The ML features are part of network-dmenu and follow the same MIT license.

## Acknowledgments

- **smartcore** team for the pure Rust ML library
- **linfa** contributors for the ML framework
- **ndarray** maintainers for numerical computing support
- All contributors who helped test and improve ML features

---

*ML features are optional and can be disabled at compile time or runtime. Traditional functionality remains fully available.*