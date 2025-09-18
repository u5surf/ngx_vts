# VTS Directives Test Documentation

## Implemented Directives

### 1. `vts_status`
- **Context**: `server`, `location`  
- **Syntax**: `vts_status;`
- **Description**: Enables VTS status endpoint at the location
- **Example**:
  ```nginx
  location /status {
      vts_status;
  }
  ```

### 2. `vts_zone`
- **Context**: `http`
- **Syntax**: `vts_zone zone_name size;`
- **Description**: Defines shared memory zone for VTS statistics
- **Example**:
  ```nginx
  vts_zone main 10m;
  ```

### 3. `vts_upstream_stats` ✅ **NEW**
- **Context**: `http`, `server`, `location`
- **Syntax**: `vts_upstream_stats on|off;`
- **Description**: Enables/disables upstream statistics collection
- **Example**:
  ```nginx
  vts_upstream_stats on;
  ```

### 4. `vts_upstream_zone` ✅ **NEW**
- **Context**: `upstream`
- **Syntax**: `vts_upstream_zone zone_name;`
- **Description**: Sets upstream zone name for statistics tracking
- **Example**:
  ```nginx
  upstream backend {
      vts_upstream_zone backend_zone;
      server 127.0.0.1:8001;
  }
  ```

## Test Status

✅ **Directives Implemented**: All 4 core VTS directives  
✅ **Build Status**: Successfully compiles  
✅ **Module Registration**: Directives registered with nginx  
⏳ **Runtime Testing**: Requires nginx integration  

## Next Steps

1. Test with real nginx instance
2. Implement directive-specific configuration storage
3. Add proper flag handling for on/off directives
4. Integrate with statistics collection system