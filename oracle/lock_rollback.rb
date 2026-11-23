# Oracle for rails-polyfill's
# `a_rolled_back_locked_save_leaves_the_record_stale_as_rails_does`.
#
# What ActiveRecord does to an optimistically locked record when an
# after_save callback raises after its UPDATE succeeded. Runs ActiveRecord
# from the OpenProject checkout's vendor/bundle, unchanged, against an
# in-memory SQLite database (the sqlite3 gem is the only extra):
#
#   GEM_HOME=<dir> gem install sqlite3
#   OPENPROJECT_SRC=<checkout> GEM_HOME=<dir> GEM_PATH=<dir> ruby oracle/lock_rollback.rb
#
# Output recorded with ActiveRecord 8.1.4.1 (OpenProject 2725194):
#   ActiveRecord 8.1.4.1
#   save raised: boom
#   after rollback: record lock_version=1 changed=["subject", "lock_version"] stored=0
#   retry: StaleObjectError
#   create rolled back: new_record=true id=nil rows=1
#   create retry: saved new_record=false rows=2
#   destroy returned: false
#   destroy rolled back: record name=b changed=["name"] stored=a destroyed=false
B = File.join(ENV.fetch("OPENPROJECT_SRC"), "vendor/bundle/ruby/4.0.0/gems")
%w[activerecord-8.1.4.1 activemodel-8.1.4.1 activesupport-8.1.4.1].each { |g| $LOAD_PATH.unshift("#{B}/#{g}/lib") }
Dir["#{B}/{concurrent-ruby,i18n,tzinfo,minitest,base64,bigdecimal,connection_pool,drb,logger,securerandom,uri,benchmark,timeout,json}-*/lib"].each { |d| $LOAD_PATH.unshift(d) }
Dir["#{B}/concurrent-ruby-*/lib/concurrent-ruby"].each { |d| $LOAD_PATH.unshift(d) }
require "active_record"
puts "ActiveRecord #{ActiveRecord.version}"
ActiveRecord::Base.establish_connection(adapter: "sqlite3", database: ":memory:")
ActiveRecord::Schema.verbose = false
ActiveRecord::Schema.define do
  create_table(:wps) { |t| t.string :subject; t.integer :lock_version, default: 0, null: false }
  create_table(:plains) { |t| t.string :name }
end
class Wp < ActiveRecord::Base
  attr_accessor :boom
  after_save { raise "boom" if boom }
end
w = Wp.create!(subject: "a")
w.subject = "b"; w.boom = true
begin; w.save; rescue => e; puts "save raised: #{e.message}"; end
puts "after rollback: record lock_version=#{w.lock_version} changed=#{w.changed.inspect} stored=#{Wp.find(w.id).lock_version}"
w.boom = false; w.subject = "c"
begin
  w.save!; puts "retry: saved, stored lock_version=#{Wp.find(w.id).lock_version}"
rescue ActiveRecord::StaleObjectError
  puts "retry: StaleObjectError"
end
n = Wp.new(subject: "n"); n.boom = true
begin; n.save; rescue => e; end
puts "create rolled back: new_record=#{n.new_record?} id=#{n.id.inspect} rows=#{Wp.count}"
n.boom = false
n.save!
puts "create retry: saved new_record=#{n.new_record?} rows=#{Wp.count}"

# A before_destroy that saves the record, then aborts.
class Plain < ActiveRecord::Base
  before_destroy { save!; throw :abort }
end
r = Plain.create!(name: "a")
r.name = "b"
puts "destroy returned: #{r.destroy.inspect}"
puts "destroy rolled back: record name=#{r.name} changed=#{r.changed.inspect} stored=#{Plain.find(r.id).name} destroyed=#{r.destroyed?}"
